-- SERK QUORUM 0.3: Web Connect (bring your own AI) and Council.
-- Additive only. Same model as 0001: organization-scoped rows, RLS on app.org_id,
-- composite tenant foreign keys, append-only history for the application role.

-- ---------------------------------------------------------------- policy
-- Highest data classification an organization allows to leave QUORUM through
-- prepared instructions, context exports or API submissions.
ALTER TABLE organizations
    ADD COLUMN external_share_limit text NOT NULL DEFAULT 'internal'
        CHECK (external_share_limit IN ('public', 'internal', 'confidential', 'restricted'));

-- ---------------------------------------------------------------- roles
-- Organization-defined collaboration roles. The four default roles live in code
-- (quorum-domain::connect) and need no rows.
CREATE TABLE role_templates (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name            text NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
    focus           text NOT NULL DEFAULT '',
    guidance        text NOT NULL CHECK (length(guidance) BETWEEN 1 AND 4000),
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX role_templates_org_name_uniq ON role_templates (organization_id, lower(name));

-- ---------------------------------------------------------------- briefs
CREATE TABLE briefs (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL,
    project_id      uuid NOT NULL,
    title           text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    mode            text NOT NULL CHECK (mode IN ('quick', 'professional')),
    classification  text NOT NULL CHECK (classification IN ('public', 'internal', 'confidential', 'restricted')),
    current_version integer NOT NULL DEFAULT 1 CHECK (current_version >= 1),
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    UNIQUE (id, organization_id)
);
CREATE INDEX briefs_project_idx ON briefs (organization_id, project_id, updated_at DESC);

CREATE TABLE brief_versions (
    organization_id uuid NOT NULL,
    brief_id        uuid NOT NULL,
    version         integer NOT NULL CHECK (version >= 1),
    content         jsonb NOT NULL,
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (brief_id, version),
    FOREIGN KEY (brief_id, organization_id) REFERENCES briefs (id, organization_id) ON DELETE CASCADE
);

-- ---------------------------------------------------------------- handoffs
-- One prepared instruction for one provider and role. The exact outbound text is
-- stored so a response can be traced to what was actually sent.
CREATE TABLE handoffs (
    id                 uuid PRIMARY KEY,
    organization_id    uuid NOT NULL,
    project_id         uuid NOT NULL,
    brief_id           uuid NOT NULL,
    brief_version      integer NOT NULL,
    provider           text NOT NULL CHECK (provider IN ('openai', 'anthropic', 'gemini', 'xai')),
    role_name          text NOT NULL CHECK (length(role_name) BETWEEN 1 AND 60),
    mode               text NOT NULL CHECK (mode IN ('web', 'api')),
    instruction        text NOT NULL CHECK (length(instruction) BETWEEN 1 AND 60000),
    instruction_sha256 bytea NOT NULL CHECK (length(instruction_sha256) = 32),
    context_items      jsonb NOT NULL DEFAULT '[]'::jsonb,
    status             text NOT NULL DEFAULT 'prepared' CHECK (status IN ('prepared', 'opened', 'imported')),
    opened_at          timestamptz,
    created_by         uuid NOT NULL REFERENCES users (id),
    created_at         timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (brief_id, organization_id) REFERENCES briefs (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (brief_id, brief_version) REFERENCES brief_versions (brief_id, version),
    UNIQUE (id, organization_id)
);
CREATE INDEX handoffs_brief_idx ON handoffs (organization_id, brief_id, created_at DESC);

-- ---------------------------------------------------------------- responses
-- Responses from any source. provenance says who vouches for the metadata:
--   user_declared     : imported by a person; provider/model are their claim
--   provider_reported : returned by an API call QUORUM made itself
CREATE TABLE responses (
    id               uuid PRIMARY KEY,
    organization_id  uuid NOT NULL,
    project_id       uuid NOT NULL,
    handoff_id       uuid,
    brief_id         uuid,
    brief_version    integer,
    provider         text NOT NULL CHECK (provider IN ('openai', 'anthropic', 'gemini', 'xai', 'other')),
    model            text NOT NULL DEFAULT '' CHECK (length(model) <= 120),
    source_mode      text NOT NULL CHECK (source_mode IN ('web_import', 'file_import', 'api', 'fixture')),
    provenance       text NOT NULL CHECK (provenance IN ('user_declared', 'provider_reported')),
    body             text NOT NULL CHECK (length(body) BETWEEN 1 AND 200000),
    content_sha256   bytea NOT NULL CHECK (length(content_sha256) = 32),
    reference_url    text NOT NULL DEFAULT '' CHECK (reference_url = '' OR reference_url ~ '^https://'),
    review_status    text NOT NULL DEFAULT 'unreviewed' CHECK (review_status IN ('unreviewed', 'accepted', 'rejected')),
    imported_by      uuid NOT NULL REFERENCES users (id),
    imported_at      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (handoff_id, organization_id) REFERENCES handoffs (id, organization_id),
    FOREIGN KEY (brief_id, organization_id) REFERENCES briefs (id, organization_id),
    CHECK ((source_mode IN ('api', 'fixture')) = (provenance = 'provider_reported')),
    UNIQUE (id, organization_id),
    UNIQUE (project_id, content_sha256)
);
CREATE INDEX responses_project_idx ON responses (organization_id, project_id, imported_at DESC);

-- The original text and its provenance never change; only review status may.
CREATE FUNCTION responses_protect_original() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.body IS DISTINCT FROM OLD.body OR NEW.content_sha256 IS DISTINCT FROM OLD.content_sha256
       OR NEW.provider IS DISTINCT FROM OLD.provider OR NEW.model IS DISTINCT FROM OLD.model
       OR NEW.source_mode IS DISTINCT FROM OLD.source_mode OR NEW.provenance IS DISTINCT FROM OLD.provenance
       OR NEW.handoff_id IS DISTINCT FROM OLD.handoff_id OR NEW.imported_by IS DISTINCT FROM OLD.imported_by
       OR NEW.imported_at IS DISTINCT FROM OLD.imported_at OR NEW.project_id IS DISTINCT FROM OLD.project_id
       OR NEW.organization_id IS DISTINCT FROM OLD.organization_id THEN
        RAISE EXCEPTION 'response originals are immutable' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER responses_protect_original BEFORE UPDATE ON responses
    FOR EACH ROW EXECUTE FUNCTION responses_protect_original();

-- ---------------------------------------------------------------- council
CREATE TABLE councils (
    id               uuid PRIMARY KEY,
    organization_id  uuid NOT NULL,
    project_id       uuid NOT NULL,
    title            text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    question         text NOT NULL DEFAULT '',
    status           text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'awaiting_responses', 'ready_for_review',
                                                                     'under_review', 'decision_recorded', 'archived')),
    decision_summary text NOT NULL DEFAULT '',
    decision_id      uuid,
    version          integer NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by       uuid NOT NULL REFERENCES users (id),
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (decision_id, organization_id) REFERENCES decisions (id, organization_id),
    UNIQUE (id, organization_id)
);
CREATE INDEX councils_project_idx ON councils (organization_id, project_id, updated_at DESC);

CREATE TABLE council_responses (
    organization_id uuid NOT NULL,
    council_id      uuid NOT NULL,
    response_id     uuid NOT NULL,
    position        smallint NOT NULL CHECK (position BETWEEN 1 AND 12),
    added_by        uuid NOT NULL REFERENCES users (id),
    added_at        timestamptz NOT NULL DEFAULT now(),
    removed_at      timestamptz,
    replaced_by     uuid,
    PRIMARY KEY (council_id, response_id),
    FOREIGN KEY (council_id, organization_id) REFERENCES councils (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (response_id, organization_id) REFERENCES responses (id, organization_id)
);
-- One active response per position.
CREATE UNIQUE INDEX council_responses_active_slot ON council_responses (council_id, position) WHERE removed_at IS NULL;

CREATE TABLE council_annotations (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL,
    council_id      uuid NOT NULL,
    response_id     uuid,
    kind            text NOT NULL CHECK (kind IN ('note', 'highlight', 'evidence', 'assumption', 'contradiction',
                                                  'alternative', 'question', 'risk')),
    quote           text NOT NULL DEFAULT '' CHECK (length(quote) <= 4000),
    body            text NOT NULL DEFAULT '' CHECK (length(body) <= 8000),
    author_id       uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (council_id, organization_id) REFERENCES councils (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (response_id, organization_id) REFERENCES responses (id, organization_id),
    CHECK (quote <> '' OR body <> '')
);
CREATE INDEX council_annotations_idx ON council_annotations (organization_id, council_id, created_at);

CREATE TABLE council_ratings (
    organization_id uuid NOT NULL,
    council_id      uuid NOT NULL,
    response_id     uuid NOT NULL,
    criterion       text NOT NULL CHECK (criterion IN ('evidence_quality', 'requirements_coverage', 'technical_feasibility',
                                                       'clarity', 'cost_implications', 'maintainability',
                                                       'identified_risks', 'verification_status')),
    score           smallint NOT NULL CHECK (score BETWEEN 1 AND 5),
    reason          text NOT NULL DEFAULT '' CHECK (length(reason) <= 2000),
    rated_by        uuid NOT NULL REFERENCES users (id),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (council_id, response_id, criterion, rated_by),
    FOREIGN KEY (council_id, organization_id) REFERENCES councils (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (response_id, organization_id) REFERENCES responses (id, organization_id)
);

-- Completed councils are frozen in the database as well as in the application.
CREATE FUNCTION councils_protect_completed() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF OLD.status IN ('decision_recorded', 'archived') AND (
        NEW.title IS DISTINCT FROM OLD.title OR NEW.question IS DISTINCT FROM OLD.question
        OR NEW.decision_summary IS DISTINCT FROM OLD.decision_summary OR NEW.decision_id IS DISTINCT FROM OLD.decision_id
        OR NEW.project_id IS DISTINCT FROM OLD.project_id) THEN
        RAISE EXCEPTION 'council is completed' USING ERRCODE = 'check_violation';
    END IF;
    IF OLD.status = 'archived' AND NEW.status IS DISTINCT FROM OLD.status THEN
        RAISE EXCEPTION 'archived councils are final' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER councils_protect_completed BEFORE UPDATE ON councils
    FOR EACH ROW EXECUTE FUNCTION councils_protect_completed();

-- ---------------------------------------------------------------- RLS
ALTER TABLE role_templates      ENABLE ROW LEVEL SECURITY;
ALTER TABLE briefs              ENABLE ROW LEVEL SECURITY;
ALTER TABLE brief_versions      ENABLE ROW LEVEL SECURITY;
ALTER TABLE handoffs            ENABLE ROW LEVEL SECURITY;
ALTER TABLE responses           ENABLE ROW LEVEL SECURITY;
ALTER TABLE councils            ENABLE ROW LEVEL SECURITY;
ALTER TABLE council_responses   ENABLE ROW LEVEL SECURITY;
ALTER TABLE council_annotations ENABLE ROW LEVEL SECURITY;
ALTER TABLE council_ratings     ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON role_templates      USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON briefs              USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON brief_versions      USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON handoffs            USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON responses           USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON councils            USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON council_responses   USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON council_annotations USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON council_ratings     USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());

-- ---------------------------------------------------------------- grants
GRANT SELECT, INSERT, UPDATE, DELETE ON role_templates, council_annotations, council_ratings TO quorum_app;
GRANT SELECT, INSERT, UPDATE ON briefs, handoffs, responses, councils, council_responses TO quorum_app;
GRANT SELECT, INSERT ON brief_versions TO quorum_app;
