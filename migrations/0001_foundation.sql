-- SERK QUORUM 0.2 foundation schema.
--
-- Run by the migration role (owner of all objects). The API connects as a
-- separate login role that is a member of `quorum_app`, which:
--   * does not own any table, so row-level security applies to it;
--   * has only the grants listed at the end of this file;
--   * cannot UPDATE or DELETE audit events or document/decision versions.
--
-- Tenant context is supplied per transaction with
--   SELECT set_config('app.user_id', <uuid>, true), set_config('app.org_id', <uuid>, true);
-- `true` makes the setting transaction-local, so it cannot leak to the next
-- request that reuses a pooled connection.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'quorum_app') THEN
        CREATE ROLE quorum_app NOLOGIN;
    END IF;
END
$$;

CREATE FUNCTION app_current_user() RETURNS uuid
    LANGUAGE sql STABLE
    AS $$ SELECT nullif(current_setting('app.user_id', true), '')::uuid $$;

CREATE FUNCTION app_current_org() RETURNS uuid
    LANGUAGE sql STABLE
    AS $$ SELECT nullif(current_setting('app.org_id', true), '')::uuid $$;

-- ---------------------------------------------------------------- identity

CREATE TABLE users (
    id               uuid PRIMARY KEY,
    email            text NOT NULL,
    email_normalized text NOT NULL UNIQUE CHECK (email_normalized = lower(email_normalized)),
    display_name     text NOT NULL CHECK (length(display_name) BETWEEN 1 AND 120),
    -- NULL for accounts that authenticate only through an external identity provider.
    password_hash    text,
    status           text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'deactivated')),
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id           uuid PRIMARY KEY,
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- SHA-256 of the bearer secret; the secret itself is never stored.
    token_digest bytea NOT NULL UNIQUE CHECK (length(token_digest) = 32),
    csrf_token   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    revoked_at   timestamptz,
    CHECK (expires_at > created_at)
);
CREATE INDEX sessions_user_idx ON sessions (user_id) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------- organizations

CREATE TABLE organizations (
    id         uuid PRIMARY KEY,
    name       text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    created_by uuid NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE memberships (
    organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    user_id         uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role            text NOT NULL CHECK (role IN ('owner', 'administrator', 'project_manager', 'contributor',
                                                  'reviewer', 'billing_administrator', 'auditor', 'read_only')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, user_id)
);
CREATE INDEX memberships_user_idx ON memberships (user_id);

CREATE TABLE invitations (
    id               uuid PRIMARY KEY,
    organization_id  uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    email_normalized text NOT NULL,
    role             text NOT NULL CHECK (role IN ('owner', 'administrator', 'project_manager', 'contributor',
                                                   'reviewer', 'billing_administrator', 'auditor', 'read_only')),
    token_digest     bytea NOT NULL UNIQUE CHECK (length(token_digest) = 32),
    invited_by       uuid NOT NULL REFERENCES users (id),
    created_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    accepted_at      timestamptz,
    accepted_by      uuid REFERENCES users (id),
    revoked_at       timestamptz
);
CREATE INDEX invitations_org_idx ON invitations (organization_id, created_at DESC);

-- --------------------------------------------------------------- projects

CREATE TABLE projects (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name            text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    description     text NOT NULL DEFAULT '',
    status          text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'on_hold', 'completed', 'archived')),
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    -- Composite key lets children prove they belong to the same tenant.
    UNIQUE (id, organization_id)
);
-- Tenant-scoped uniqueness: two organizations may both have "Atlas".
CREATE UNIQUE INDEX projects_org_name_uniq ON projects (organization_id, lower(name));

-- -------------------------------------------------------------- documents

CREATE TABLE documents (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL,
    project_id      uuid NOT NULL,
    title           text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    category        text NOT NULL,
    status          text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved')),
    current_version integer NOT NULL DEFAULT 1 CHECK (current_version >= 1),
    approved_version integer,
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    UNIQUE (id, organization_id),
    CHECK (approved_version IS NULL OR approved_version <= current_version)
);
CREATE INDEX documents_project_idx ON documents (organization_id, project_id, updated_at DESC);

-- Append-only: the application role may INSERT and SELECT, never UPDATE or DELETE.
CREATE TABLE document_versions (
    organization_id uuid NOT NULL,
    document_id     uuid NOT NULL,
    version         integer NOT NULL CHECK (version >= 1),
    title           text NOT NULL,
    body            text NOT NULL,
    ai_assisted     boolean NOT NULL DEFAULT false,
    change_note     text NOT NULL DEFAULT '',
    created_by      uuid NOT NULL REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (document_id, version),
    FOREIGN KEY (document_id, organization_id) REFERENCES documents (id, organization_id) ON DELETE CASCADE
);

-- -------------------------------------------------------------- decisions

CREATE TABLE decisions (
    id                  uuid PRIMARY KEY,
    organization_id     uuid NOT NULL,
    project_id          uuid NOT NULL,
    number              integer NOT NULL CHECK (number >= 1),
    title               text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    problem_statement   text NOT NULL,
    context             text NOT NULL DEFAULT '',
    alternatives        text NOT NULL DEFAULT '',
    evaluation_criteria text NOT NULL DEFAULT '',
    evidence            text NOT NULL DEFAULT '',
    outcome             text NOT NULL DEFAULT '',
    consequences        text NOT NULL DEFAULT '',
    risks               text NOT NULL DEFAULT '',
    decision_maker_id   uuid REFERENCES users (id),
    review_date         date,
    status              text NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft', 'under_review', 'approved', 'rejected', 'superseded')),
    superseded_by       uuid,
    version             integer NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by          uuid NOT NULL REFERENCES users (id),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE CASCADE,
    UNIQUE (id, organization_id),
    UNIQUE (project_id, number),
    -- MATCH SIMPLE: checked only when superseded_by is set; must be in the same tenant.
    FOREIGN KEY (superseded_by, organization_id) REFERENCES decisions (id, organization_id),
    CHECK ((status = 'superseded') = (superseded_by IS NOT NULL)),
    CHECK (superseded_by IS NULL OR superseded_by <> id)
);
CREATE INDEX decisions_project_idx ON decisions (organization_id, project_id, number DESC);

-- Approved and superseded decisions keep their content: the database itself
-- refuses content changes, independently of application checks.
CREATE FUNCTION decisions_protect_content() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF OLD.status IN ('approved', 'superseded', 'rejected', 'under_review') AND (
        NEW.title IS DISTINCT FROM OLD.title OR
        NEW.problem_statement IS DISTINCT FROM OLD.problem_statement OR
        NEW.context IS DISTINCT FROM OLD.context OR
        NEW.alternatives IS DISTINCT FROM OLD.alternatives OR
        NEW.evaluation_criteria IS DISTINCT FROM OLD.evaluation_criteria OR
        NEW.evidence IS DISTINCT FROM OLD.evidence OR
        NEW.outcome IS DISTINCT FROM OLD.outcome OR
        NEW.consequences IS DISTINCT FROM OLD.consequences OR
        NEW.risks IS DISTINCT FROM OLD.risks OR
        NEW.project_id IS DISTINCT FROM OLD.project_id OR
        NEW.organization_id IS DISTINCT FROM OLD.organization_id
    ) THEN
        RAISE EXCEPTION 'decision content is immutable in status %', OLD.status
            USING ERRCODE = 'check_violation';
    END IF;
    IF OLD.status = 'superseded' AND NEW.status IS DISTINCT FROM OLD.status THEN
        RAISE EXCEPTION 'superseded decisions are final' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER decisions_protect_content BEFORE UPDATE ON decisions
    FOR EACH ROW EXECUTE FUNCTION decisions_protect_content();

CREATE TABLE decision_versions (
    organization_id uuid NOT NULL,
    decision_id     uuid NOT NULL,
    version         integer NOT NULL CHECK (version >= 1),
    status          text NOT NULL,
    snapshot        jsonb NOT NULL,
    note            text NOT NULL DEFAULT '',
    changed_by      uuid NOT NULL REFERENCES users (id),
    changed_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (decision_id, version),
    FOREIGN KEY (decision_id, organization_id) REFERENCES decisions (id, organization_id) ON DELETE CASCADE
);

-- -------------------------------------------------------- provider usage

-- Metadata only. The submitted text is not stored here.
CREATE TABLE provider_requests (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    user_id         uuid NOT NULL REFERENCES users (id),
    project_id      uuid,
    provider        text NOT NULL CHECK (provider IN ('openai', 'anthropic', 'gemini', 'xai')),
    model           text NOT NULL,
    task            text NOT NULL,
    mode            text NOT NULL CHECK (mode IN ('fixture', 'live')),
    outcome         text NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
    prompt_chars    integer NOT NULL CHECK (prompt_chars >= 0),
    input_tokens    integer CHECK (input_tokens >= 0),
    output_tokens   integer CHECK (output_tokens >= 0),
    latency_ms      integer NOT NULL CHECK (latency_ms >= 0),
    created_at      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (project_id, organization_id) REFERENCES projects (id, organization_id) ON DELETE SET NULL (project_id)
);
CREATE INDEX provider_requests_org_idx ON provider_requests (organization_id, created_at DESC);

-- ------------------------------------------------------------------ audit

-- Append-only for the application role (INSERT, SELECT only). This is not
-- tamper-proof against the database owner; see docs/security.
CREATE TABLE audit_events (
    id              uuid PRIMARY KEY,
    organization_id uuid REFERENCES organizations (id) ON DELETE CASCADE,
    actor_user_id   uuid REFERENCES users (id) ON DELETE SET NULL,
    event_type      text NOT NULL CHECK (event_type ~ '^[a-z_]+(\.[a-z_]+)+$'),
    resource_type   text,
    resource_id     uuid,
    outcome         text NOT NULL CHECK (outcome IN ('success', 'denied', 'failure')),
    correlation_id  text NOT NULL,
    details         jsonb NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_events_org_idx ON audit_events (organization_id, occurred_at DESC);

-- ------------------------------------------------ authentication lookups
--
-- These run before any tenant context exists (login, session validation,
-- invitation lookup). They are SECURITY DEFINER so they can read through
-- row-level security, and each returns only the minimum needed.

CREATE FUNCTION auth_find_user_by_email(p_email text)
    RETURNS TABLE (id uuid, password_hash text, status text)
    LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public
    AS $$ SELECT u.id, u.password_hash, u.status FROM users u WHERE u.email_normalized = p_email $$;

CREATE FUNCTION auth_find_session(p_digest bytea)
    RETURNS TABLE (session_id uuid, user_id uuid, csrf_token text, expires_at timestamptz)
    LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public
    AS $$
        SELECT s.id, s.user_id, s.csrf_token, s.expires_at
        FROM sessions s JOIN users u ON u.id = s.user_id
        WHERE s.token_digest = p_digest
          AND s.revoked_at IS NULL
          AND s.expires_at > now()
          AND u.status = 'active'
    $$;

CREATE FUNCTION auth_find_invitation(p_digest bytea)
    RETURNS TABLE (id uuid, organization_id uuid, email_normalized text, role text)
    LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public
    AS $$
        SELECT i.id, i.organization_id, i.email_normalized, i.role
        FROM invitations i
        WHERE i.token_digest = p_digest
          AND i.accepted_at IS NULL
          AND i.revoked_at IS NULL
          AND i.expires_at > now()
    $$;

CREATE FUNCTION auth_email_exists(p_email text) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public
    AS $$ SELECT EXISTS (SELECT 1 FROM users WHERE email_normalized = p_email) $$;

REVOKE ALL ON FUNCTION auth_find_user_by_email(text), auth_find_session(bytea),
                       auth_find_invitation(bytea), auth_email_exists(text) FROM PUBLIC;

-- ------------------------------------------------------ row-level security

ALTER TABLE users             ENABLE ROW LEVEL SECURITY;
ALTER TABLE sessions          ENABLE ROW LEVEL SECURITY;
ALTER TABLE organizations     ENABLE ROW LEVEL SECURITY;
ALTER TABLE memberships       ENABLE ROW LEVEL SECURITY;
ALTER TABLE invitations       ENABLE ROW LEVEL SECURITY;
ALTER TABLE projects          ENABLE ROW LEVEL SECURITY;
ALTER TABLE documents         ENABLE ROW LEVEL SECURITY;
ALTER TABLE document_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE decisions         ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE provider_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_events      ENABLE ROW LEVEL SECURITY;

-- A user sees their own row and the rows of members of the current organization.
CREATE POLICY users_select ON users FOR SELECT USING (
    id = app_current_user()
    OR EXISTS (SELECT 1 FROM memberships m
               WHERE m.user_id = users.id AND m.organization_id = app_current_org())
);
-- Signup inserts the new user with app.user_id already set to the new id.
CREATE POLICY users_insert ON users FOR INSERT WITH CHECK (id = app_current_user());
CREATE POLICY users_update ON users FOR UPDATE USING (id = app_current_user());

CREATE POLICY sessions_own ON sessions USING (user_id = app_current_user())
    WITH CHECK (user_id = app_current_user());

CREATE POLICY organizations_select ON organizations FOR SELECT USING (
    id = app_current_org()
    OR EXISTS (SELECT 1 FROM memberships m WHERE m.organization_id = organizations.id
                                             AND m.user_id = app_current_user())
);
CREATE POLICY organizations_insert ON organizations FOR INSERT
    WITH CHECK (id = app_current_org() AND created_by = app_current_user());
CREATE POLICY organizations_update ON organizations FOR UPDATE USING (id = app_current_org());

CREATE POLICY memberships_select ON memberships FOR SELECT
    USING (organization_id = app_current_org() OR user_id = app_current_user());
CREATE POLICY memberships_write ON memberships FOR ALL
    USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());

CREATE POLICY tenant_isolation ON invitations       USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON projects          USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON documents         USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON document_versions USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON decisions         USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON decision_versions USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());
CREATE POLICY tenant_isolation ON provider_requests USING (organization_id = app_current_org()) WITH CHECK (organization_id = app_current_org());

CREATE POLICY audit_select ON audit_events FOR SELECT USING (organization_id = app_current_org());
CREATE POLICY audit_insert ON audit_events FOR INSERT WITH CHECK (
    organization_id IS NULL OR organization_id = app_current_org()
);

-- ------------------------------------------------------------------ grants

GRANT USAGE ON SCHEMA public TO quorum_app;
GRANT SELECT, INSERT, UPDATE ON users, sessions, organizations TO quorum_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON memberships, invitations, projects, documents, decisions TO quorum_app;
GRANT SELECT, INSERT ON document_versions, decision_versions, provider_requests, audit_events TO quorum_app;
GRANT EXECUTE ON FUNCTION app_current_user(), app_current_org() TO quorum_app;
GRANT EXECUTE ON FUNCTION auth_find_user_by_email(text), auth_find_session(bytea),
                          auth_find_invitation(bytea), auth_email_exists(text) TO quorum_app;
