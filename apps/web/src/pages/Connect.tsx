import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Link, useNavigate, useOutletContext, useSearchParams } from "react-router";
import { ApiError, api, errorMessage } from "../api/client";
import {
  CLASSIFICATION_LABELS,
  EMPTY_BRIEF,
  type AiResponse,
  type Brief,
  type BriefContent,
  type BriefMode,
  type Classification,
  type ConnectCatalog,
  type CouncilDetail,
  type Decision,
  type DocumentSummary,
  type Handoff,
  type Membership,
  type Paged,
  type Preview,
  type Project,
  type ProviderId,
} from "../api/types";
import { Empty, ErrorNotice, Field, Loading, StatusBadge } from "../components/ui";
import { formatDate, plural, useLoad } from "../hooks";
import { can } from "../session";

/** QUORUM Connect — Bring Your Own AI. Works without any API key: the person
 *  copies the prepared instruction, opens the provider's official website
 *  themselves, and pastes the answer back. QUORUM never automates or reads
 *  the external website. */
export function ConnectPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const [params, setParams] = useSearchParams();
  const projectId = params.get("project") ?? "";
  const briefId = params.get("brief") ?? "";
  const catalog = useLoad((s) => api.get<ConnectCatalog>(`/orgs/${org}/connect`, s), [org]);
  const projects = useLoad((s) => api.get<Paged<Project>>(`/orgs/${org}/projects?limit=100`, s), [org]);

  if (catalog.loading && !catalog.data) return <Loading />;
  if (catalog.error || !catalog.data) return <ErrorNotice message={catalog.error ?? "Unavailable"} onRetry={catalog.reload} />;
  const open = projects.data?.items.filter((p) => p.status !== "archived") ?? [];

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">QUORUM Connect · Web Connect</span>
          <h1>Bring your own AI</h1>
          <p>
            Prepare one brief, give each AI a role, then use your own ChatGPT, Claude, Gemini or Grok account in your browser.
            No API key is needed.
          </p>
        </div>
      </div>
      <div className="notice" role="note">
        <strong>How this works.</strong> QUORUM prepares the text. You copy it, open the provider website yourself and paste
        the answer back. {catalog.data.notice}
      </div>

      <section className="card stack" aria-labelledby="step-project">
        <h2 id="step-project">1. Project</h2>
        {projects.loading ? (
          <Loading />
        ) : open.length === 0 ? (
          <Empty title="No active projects">
            <Link to="../projects" relative="path">
              Create a project first.
            </Link>
          </Empty>
        ) : (
          <Field label="Project">
            {(p) => (
              <select {...p} value={projectId} onChange={(e) => setParams(e.target.value ? { project: e.target.value } : {})}>
                <option value="">Choose a project…</option>
                {open.map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name}
                  </option>
                ))}
              </select>
            )}
          </Field>
        )}
      </section>

      {projectId && (
        <Briefs
          key={projectId}
          m={m}
          catalog={catalog.data}
          projectId={projectId}
          briefId={briefId}
          onSelect={(b) => setParams(b ? { project: projectId, brief: b } : { project: projectId })}
        />
      )}
    </>
  );
}

function Briefs({
  m,
  catalog,
  projectId,
  briefId,
  onSelect,
}: {
  m: Membership;
  catalog: ConnectCatalog;
  projectId: string;
  briefId: string;
  onSelect: (id: string | null) => void;
}) {
  const org = m.organization_id;
  const list = useLoad((s) => api.get<Paged<Brief>>(`/orgs/${org}/projects/${projectId}/briefs?limit=100`, s), [org, projectId]);
  const [composing, setComposing] = useState(false);
  const selected = list.data?.items.find((b) => b.id === briefId);
  const writable = can(m, "providers_use");

  return (
    <>
      <section className="card stack" aria-labelledby="step-brief">
        <div className="row-between">
          <h2 id="step-brief">2. Brief</h2>
          {writable && !composing && (
            <button type="button" className="btn primary" onClick={() => setComposing(true)}>
              New brief
            </button>
          )}
        </div>
        {composing && (
          <BriefEditor
            org={org}
            projectId={projectId}
            catalog={catalog}
            onCancel={() => setComposing(false)}
            onSaved={(b) => {
              setComposing(false);
              list.reload();
              onSelect(b.id);
            }}
          />
        )}
        {list.loading && !list.data ? (
          <Loading />
        ) : list.error ? (
          <ErrorNotice message={list.error} onRetry={list.reload} />
        ) : list.data?.items.length === 0 ? (
          !composing && <Empty title="No briefs yet">A brief states the request once, for every AI you consult.</Empty>
        ) : (
          <ul className="list" aria-label="Briefs">
            {list.data?.items.map((b) => (
              <li key={b.id}>
                <button
                  type="button"
                  className="list-link"
                  aria-current={b.id === briefId ? "true" : undefined}
                  onClick={() => onSelect(b.id === briefId ? null : b.id)}
                >
                  <span>
                    <span className="title">{b.title}</span>
                    <br />
                    <span className="small muted">
                      {b.mode === "quick" ? "Quick" : "Professional"} · v{b.version} · updated {formatDate(b.updated_at)}
                    </span>
                  </span>
                  <StatusBadge status={b.classification} label={CLASSIFICATION_LABELS[b.classification]} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {selected && (
        <BriefWorkspace key={`${selected.id}-${selected.version}`} m={m} catalog={catalog} brief={selected} onChanged={list.reload} />
      )}
    </>
  );
}

const PROFESSIONAL_FIELDS: [keyof BriefContent, string, string][] = [
  ["current_situation", "Current situation", "What exists today."],
  ["desired_outcome", "Desired outcome", "What good looks like."],
  ["background", "Background", ""],
  ["constraints", "Constraints", "Budget, schedule, standards, technology."],
  ["requirements", "Relevant requirements", ""],
  ["deliverable_format", "Deliverable format", "For example: a comparison table and a recommendation."],
  ["evaluation_criteria", "Evaluation criteria", "How answers will be judged."],
  ["open_questions", "Open questions", ""],
];

function BriefEditor({
  org,
  projectId,
  catalog,
  brief,
  onSaved,
  onCancel,
}: {
  org: string;
  projectId: string;
  catalog: ConnectCatalog;
  brief?: Brief;
  onSaved: (b: Brief) => void;
  onCancel: () => void;
}) {
  const [title, setTitle] = useState(brief?.title ?? "");
  const [mode, setMode] = useState<BriefMode>(brief?.mode ?? "quick");
  const [classification, setClassification] = useState<Classification>(brief?.classification ?? "internal");
  const [content, setContent] = useState<BriefContent>({ ...EMPTY_BRIEF, ...(brief?.content ?? {}) });
  const [error, setError] = useState<ApiError | string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (k: keyof BriefContent, v: string) => setContent((c) => ({ ...c, [k]: v }));
  const fieldError = (f: string) => (error instanceof ApiError && error.field === f ? error.message : null);

  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const body = { title, mode, classification, content };
    try {
      const b = brief
        ? await api.patch<Brief>(`/orgs/${org}/briefs/${brief.id}`, { ...body, base_version: brief.version })
        : await api.post<Brief>(`/orgs/${org}/projects/${projectId}/briefs`, body);
      onSaved(b);
    } catch (err) {
      setError(err instanceof ApiError ? err : errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  const overLimit = catalog.classifications.indexOf(classification) > catalog.classifications.indexOf(catalog.external_share_limit);

  return (
    <form className="stack card" onSubmit={save} aria-label={brief ? "Edit brief" : "New brief"}>
      {error && <ErrorNotice message={typeof error === "string" ? error : error.message} />}
      <fieldset className="segmented" aria-label="Brief mode">
        {(["quick", "professional"] as BriefMode[]).map((x) => (
          <label key={x}>
            <input type="radio" name="brief-mode" value={x} checked={mode === x} onChange={() => setMode(x)} />
            {x === "quick" ? "Quick" : "Professional"}
          </label>
        ))}
      </fieldset>
      <div className="grid grid-2">
        <Field label="Title" error={fieldError("title")}>
          {(p) => <input {...p} type="text" required maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} />}
        </Field>
        <Field
          label="Classification"
          hint={`Your organization allows ${CLASSIFICATION_LABELS[catalog.external_share_limit]} material or lower to go to external AI.`}
        >
          {(p) => (
            <select {...p} value={classification} onChange={(e) => setClassification(e.target.value as Classification)}>
              {catalog.classifications.map((c) => (
                <option key={c} value={c}>
                  {CLASSIFICATION_LABELS[c]}
                </option>
              ))}
            </select>
          )}
        </Field>
      </div>
      {overLimit && (
        <div className="notice warn" role="status">
          {CLASSIFICATION_LABELS[classification]} briefs can be saved but cannot be shared with external AI providers under your
          organization's policy.
        </div>
      )}
      <Field
        label={mode === "quick" ? "What do you need?" : "Objective"}
        hint="Use synthetic or approved information only. Do not paste secrets or credentials."
        error={fieldError("request")}
      >
        {(p) => <textarea {...p} required rows={5} value={content.request} onChange={(e) => set("request", e.target.value)} />}
      </Field>
      {mode === "professional" && (
        <div className="grid grid-2">
          {PROFESSIONAL_FIELDS.map(([k, label, hint]) => (
            <Field key={k} label={label} {...(hint ? { hint } : {})} error={fieldError(k)}>
              {(p) => <textarea {...p} rows={3} value={content[k]} onChange={(e) => set(k, e.target.value)} />}
            </Field>
          ))}
          <Field label="Intended audience" error={fieldError("audience")}>
            {(p) => <input {...p} type="text" maxLength={200} value={content.audience} onChange={(e) => set("audience", e.target.value)} />}
          </Field>
          <Field label="Answer language" hint="For example: English" error={fieldError("language")}>
            {(p) => <input {...p} type="text" maxLength={40} value={content.language} onChange={(e) => set("language", e.target.value)} />}
          </Field>
        </div>
      )}
      <div className="row">
        <button className="btn primary" type="submit" disabled={busy || !title.trim() || !content.request.trim()}>
          {busy ? "Saving…" : brief ? "Save new version" : "Save brief"}
        </button>
        <button className="btn ghost" type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

interface Assignment {
  provider: ProviderId;
  role: string;
  include: boolean;
}

function BriefWorkspace({
  m,
  catalog,
  brief,
  onChanged,
}: {
  m: Membership;
  catalog: ConnectCatalog;
  brief: Brief;
  onChanged: () => void;
}) {
  const org = m.organization_id;
  const navigate = useNavigate();
  const [editing, setEditing] = useState(false);
  const [assignments, setAssignments] = useState<Assignment[]>(() =>
    catalog.providers.map((p) => ({
      provider: p.id,
      role: catalog.roles.find((r) => r.suggested_provider === p.id)?.key ?? catalog.roles[0]?.key ?? "",
      include: true,
    })),
  );
  const [contextIds, setContextIds] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<Preview | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const handoffs = useLoad((s) => api.get<Handoff[]>(`/orgs/${org}/briefs/${brief.id}/handoffs`, s), [org, brief.id]);
  const responses = useLoad(
    (s) => api.get<Paged<AiResponse>>(`/orgs/${org}/projects/${brief.project_id}/responses?brief=${brief.id}&limit=100`, s),
    [org, brief.id],
  );
  const docs = useLoad(
    (s) => api.get<Paged<DocumentSummary>>(`/orgs/${org}/projects/${brief.project_id}/documents?limit=100`, s),
    [org, brief.project_id],
  );
  const decisions = useLoad(
    (s) => api.get<Paged<Decision>>(`/orgs/${org}/projects/${brief.project_id}/decisions?limit=100`, s),
    [org, brief.project_id],
  );
  const context = useMemo(
    () =>
      [...contextIds].map((key) => {
        const [kind, id] = key.split(":");
        return { kind: kind as "document" | "decision", id: id ?? "" };
      }),
    [contextIds],
  );
  const writable = can(m, "providers_use");
  const product = (id: ProviderId) => catalog.providers.find((p) => p.id === id)?.web_product ?? id;

  async function prepare() {
    setBusy(true);
    setError(null);
    try {
      const chosen = assignments.filter((a) => a.include).map(({ provider, role }) => ({ provider, role }));
      const p = await api.post<Preview>(`/orgs/${org}/briefs/${brief.id}/preview`, { assignments: chosen, context });
      setPreview(p);
      setDrafts(Object.fromEntries(p.instructions.map((i) => [`${i.provider}:${i.role}`, i.instruction])));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function approve(provider: ProviderId, role: string, original: string) {
    setError(null);
    const text = drafts[`${provider}:${role}`] ?? original;
    try {
      await api.post<Handoff>(`/orgs/${org}/briefs/${brief.id}/handoffs`, {
        provider,
        role,
        base_version: brief.version,
        context,
        ...(text !== original ? { instruction: text } : {}),
      });
      setPreview((p) =>
        p ? { ...p, instructions: p.instructions.filter((i) => !(i.provider === provider && i.role === role)) } : p,
      );
      handoffs.reload();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function compare() {
    setError(null);
    // Positions follow the order the answers were imported.
    const ids = (responses.data?.items ?? [])
      .filter((r) => r.review_status !== "rejected")
      .sort((a, b) => a.imported_at.localeCompare(b.imported_at))
      .map((r) => r.id);
    try {
      const c = await api.post<CouncilDetail>(`/orgs/${org}/projects/${brief.project_id}/councils`, {
        title: brief.title,
        question: brief.content.request ?? "",
        response_ids: ids,
      });
      navigate(`../councils/${c.id}`, { relative: "path" });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  if (editing) {
    return (
      <section className="card stack" aria-label="Edit brief">
        <BriefEditor
          org={org}
          projectId={brief.project_id}
          catalog={catalog}
          brief={brief}
          onCancel={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            onChanged();
          }}
        />
      </section>
    );
  }

  const importedCount = responses.data?.items.length ?? 0;

  return (
    <>
      <section className="card stack" aria-labelledby="step-roles">
        <div className="row-between">
          <h2 id="step-roles">3. Roles and instructions</h2>
          {writable && (
            <button type="button" className="btn small" onClick={() => setEditing(true)}>
              Edit brief
            </button>
          )}
        </div>
        <p className="small muted">
          Brief “{brief.title}”, version {brief.version}. Each instruction states one role. Roles are a convention: any provider can
          take any role.
        </p>
        {error && <ErrorNotice message={error} />}
        <div className="grid grid-4">
          {assignments.map((a, i) => (
            <fieldset key={a.provider} className="card stack">
              <legend>{product(a.provider)}</legend>
              <label className="check">
                <input
                  type="checkbox"
                  checked={a.include}
                  onChange={(e) => setAssignments((xs) => xs.map((x, j) => (j === i ? { ...x, include: e.target.checked } : x)))}
                />
                Include
              </label>
              <Field label={`Role for ${product(a.provider)}`}>
                {(p) => (
                  <select
                    {...p}
                    value={a.role}
                    disabled={!a.include}
                    onChange={(e) => setAssignments((xs) => xs.map((x, j) => (j === i ? { ...x, role: e.target.value } : x)))}
                  >
                    {catalog.roles.map((r) => (
                      <option key={r.key} value={r.key}>
                        {r.name}
                      </option>
                    ))}
                  </select>
                )}
              </Field>
            </fieldset>
          ))}
        </div>
        <details>
          <summary>Approved project context ({contextIds.size} selected)</summary>
          <p className="small muted">Only what you select is included, quoted as reference material with its version.</p>
          <ul className="stack" aria-label="Context items">
            {[
              ...(docs.data?.items ?? []).map((d) => ({ key: `document:${d.id}`, label: `${d.title} (document v${d.current_version})` })),
              ...(decisions.data?.items ?? []).map((d) => ({ key: `decision:${d.id}`, label: `${d.reference} ${d.title} (decision)` })),
            ].map((x) => (
              <li key={x.key}>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={contextIds.has(x.key)}
                    onChange={(e) =>
                      setContextIds((s) => {
                        const n = new Set(s);
                        if (e.target.checked) n.add(x.key);
                        else n.delete(x.key);
                        return n;
                      })
                    }
                  />
                  {x.label}
                </label>
              </li>
            ))}
          </ul>
        </details>
        {writable && (
          <div className="row">
            <button type="button" className="btn primary" disabled={busy || !assignments.some((a) => a.include)} onClick={() => void prepare()}>
              {busy ? "Preparing…" : "Prepare instructions"}
            </button>
          </div>
        )}
        {preview && !preview.sharing_allowed && (
          <div className="notice error" role="alert">
            This brief is {CLASSIFICATION_LABELS[preview.classification]}. Your organization allows only{" "}
            {CLASSIFICATION_LABELS[preview.external_share_limit]} material or lower to be shared with external AI providers, so it
            cannot be handed off.
          </div>
        )}
        {preview?.sharing_allowed &&
          preview.instructions.map((i) => {
            const key = `${i.provider}:${i.role}`;
            const text = drafts[key] ?? i.instruction;
            return (
              <article key={key} className="card stack" aria-label={`Instruction for ${product(i.provider)}`}>
                <div className="row-between">
                  <h3>
                    {product(i.provider)} · {i.role_name}
                  </h3>
                  <span className="small muted">
                    {text.length.toLocaleString()} characters · about {Math.ceil(text.length / 4).toLocaleString()} tokens
                  </span>
                </div>
                <Field label="Instruction to share" hint="Review and edit before approving. The approved text is recorded exactly.">
                  {(p) => (
                    <textarea {...p} rows={10} value={text} onChange={(e) => setDrafts((d) => ({ ...d, [key]: e.target.value }))} />
                  )}
                </Field>
                <div className="row">
                  <button type="button" className="btn primary" onClick={() => void approve(i.provider, i.role, i.instruction)}>
                    Approve for {product(i.provider)}
                  </button>
                </div>
              </article>
            );
          })}
      </section>

      <section className="card stack" aria-labelledby="step-handoff">
        <h2 id="step-handoff">4. Copy, open and import</h2>
        {handoffs.loading && !handoffs.data ? (
          <Loading />
        ) : handoffs.error ? (
          <ErrorNotice message={handoffs.error} onRetry={handoffs.reload} />
        ) : handoffs.data?.length === 0 ? (
          <Empty title="Nothing approved yet">Approve an instruction above to get copy, open and import steps.</Empty>
        ) : (
          <div className="grid grid-2">
            {handoffs.data?.map((h) => (
              <HandoffCard
                key={h.id}
                org={org}
                projectId={brief.project_id}
                handoff={h}
                product={product(h.provider)}
                writable={writable}
                onChanged={() => {
                  handoffs.reload();
                  responses.reload();
                }}
              />
            ))}
          </div>
        )}
      </section>

      <section className="card stack" aria-labelledby="step-council">
        <div className="row-between">
          <h2 id="step-council">5. Compare in Council</h2>
          {can(m, "decisions_write") && (
            <button type="button" className="btn primary" disabled={importedCount < 2} onClick={() => void compare()}>
              Compare {plural(importedCount, "response")}
            </button>
          )}
        </div>
        {importedCount < 2 && <p className="small muted">Import at least two responses to compare them.</p>}
        <ul className="list" aria-label="Imported responses">
          {responses.data?.items.map((r) => (
            <li key={r.id} className="row-between pad-y">
              <span>
                <strong>{r.role_name ?? "Response"}</strong> · {r.provider}
                {r.model ? ` / ${r.model}` : ""}
                <br />
                <span className="small muted">
                  {r.provenance === "user_declared" ? "User-declared" : "Provider-reported"} · {r.characters.toLocaleString()} characters ·
                  imported by {r.imported_by_name}
                </span>
              </span>
              <StatusBadge status={r.review_status} />
            </li>
          ))}
        </ul>
      </section>
    </>
  );
}

interface ParsedFile {
  text: string;
  provider?: string;
  model?: string;
}

/** Reads .txt/.md as text, and QUORUM JSON ({"format":"quorum.response.v1",...}). */
export function parseImportFile(name: string, raw: string): ParsedFile {
  if (name.toLowerCase().endsWith(".json")) {
    let data: unknown;
    try {
      data = JSON.parse(raw);
    } catch {
      throw new Error("This JSON file could not be read.");
    }
    const d = data as Record<string, unknown>;
    if (d.format !== "quorum.response.v1" || typeof d.text !== "string") {
      throw new Error('JSON imports must use the QUORUM format: {"format":"quorum.response.v1","text":"…"}.');
    }
    return {
      text: d.text,
      ...(typeof d.provider === "string" ? { provider: d.provider } : {}),
      ...(typeof d.model === "string" ? { model: d.model } : {}),
    };
  }
  return { text: raw };
}

const FILE_MAX_BYTES = 1_000_000;

function HandoffCard({
  org,
  projectId,
  handoff,
  product,
  writable,
  onChanged,
}: {
  org: string;
  projectId: string;
  handoff: Handoff;
  product: string;
  writable: boolean;
  onChanged: () => void;
}) {
  const [text, setText] = useState("");
  const [model, setModel] = useState("");
  const [reference, setReference] = useState("");
  const [source, setSource] = useState<"web_import" | "file_import">("web_import");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [duplicate, setDuplicate] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const area = useRef<HTMLTextAreaElement>(null);
  useEffect(() => setMessage(null), [handoff.status]);

  async function copy() {
    setError(null);
    try {
      await navigator.clipboard.writeText(handoff.instruction);
      setMessage("Instruction copied. Paste it into " + product + ".");
    } catch {
      area.current?.focus();
      area.current?.select();
      setMessage("Copying is blocked by your browser. The text is selected: press Ctrl+C or ⌘C.");
    }
  }

  function opened() {
    // Records only that the person opened the site; nothing about their account.
    if (handoff.status === "prepared") {
      void api.post(`/orgs/${org}/handoffs/${handoff.id}/opened`).then(onChanged, () => undefined);
    }
  }

  async function onFile(file: File | undefined) {
    setError(null);
    if (!file) return;
    if (file.size > FILE_MAX_BYTES) {
      setError("Files larger than 1 MB cannot be imported.");
      return;
    }
    try {
      const parsed = parseImportFile(file.name, await file.text());
      setText(parsed.text);
      if (parsed.model) setModel(parsed.model);
      setSource("file_import");
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function importResponse(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setDuplicate(null);
    try {
      await api.post<AiResponse>(`/orgs/${org}/projects/${projectId}/responses`, {
        handoff_id: handoff.id,
        provider: handoff.provider,
        model,
        text,
        source,
        reference_url: reference,
      });
      setText("");
      onChanged();
    } catch (err) {
      if (err instanceof ApiError && err.code === "duplicate_response") {
        setDuplicate(String(err.details?.existing_response_id ?? ""));
      }
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  const statusLabel =
    handoff.status === "prepared" ? "Ready to copy" : handoff.status === "opened" ? "Website opened" : "Response imported";

  return (
    <article className="card stack" aria-label={`${product} · ${handoff.role_name}`}>
      <div className="row-between">
        <h3>
          {product} · {handoff.role_name}
        </h3>
        <StatusBadge status={handoff.status === "imported" ? "completed" : "pending"} label={statusLabel} />
      </div>
      <p className="small muted">
        Brief v{handoff.brief_version} · {handoff.instruction.length.toLocaleString()} characters · SHA-256{" "}
        <code>{handoff.instruction_sha256.slice(0, 12)}…</code>
        {handoff.context_items.length > 0 && ` · ${plural(handoff.context_items.length, "context item")}`}
      </p>
      <details>
        <summary>Show instruction</summary>
        <textarea ref={area} readOnly rows={8} value={handoff.instruction} aria-label={`Instruction for ${product}`} />
      </details>
      <div className="row">
        <button type="button" className="btn" onClick={() => void copy()}>
          Copy instruction
        </button>
        {handoff.website_url && (
          <a className="btn" href={handoff.website_url} target="_blank" rel="noopener noreferrer" onClick={opened}>
            Open {product} ↗
          </a>
        )}
      </div>
      <p className="small muted" role="status" aria-live="polite">
        {message ?? "Opening the website does not send anything. Sign in there with your own account and paste the instruction."}
      </p>
      {handoff.status !== "imported" && writable && (
        <form className="stack" onSubmit={importResponse} aria-label={`Import the ${product} answer`}>
          {error && <ErrorNotice message={error} />}
          {duplicate && (
            <p className="small">
              This exact text is already saved in the project (response {duplicate.slice(0, 8)}…). Nothing was added.
            </p>
          )}
          <Field label={`Paste the ${product} answer`}>
            {(p) => (
              <textarea
                {...p}
                rows={6}
                value={text}
                onChange={(e) => {
                  setText(e.target.value);
                  setSource("web_import");
                }}
              />
            )}
          </Field>
          <div className="grid grid-2">
            <Field label="Or import a file" hint=".txt, .md or QUORUM JSON, up to 1 MB">
              {(p) => <input {...p} type="file" accept=".txt,.md,.json,text/plain,text/markdown,application/json" onChange={(e) => void onFile(e.target.files?.[0])} />}
            </Field>
            <Field label="Model (as you saw it)" hint="Optional. Recorded as your declaration.">
              {(p) => <input {...p} type="text" maxLength={120} value={model} onChange={(e) => setModel(e.target.value)} />}
            </Field>
          </div>
          <Field label="Conversation link (optional)" hint="https only. Stored for your reference; QUORUM never opens it.">
            {(p) => <input {...p} type="url" maxLength={2000} value={reference} onChange={(e) => setReference(e.target.value)} />}
          </Field>
          <div className="row">
            <button type="submit" className="btn primary" disabled={busy || !text.trim()}>
              {busy ? "Importing…" : "Import answer"}
            </button>
          </div>
        </form>
      )}
    </article>
  );
}
