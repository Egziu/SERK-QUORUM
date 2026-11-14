import { useRef, useState, type FormEvent } from "react";
import { Link, useOutletContext } from "react-router";
import { api, errorMessage } from "../api/client";
import type {
  Catalog,
  DocumentDetail,
  Membership,
  Mode,
  Paged,
  Project,
  ProviderId,
  RunResult,
  Task,
} from "../api/types";
import { ErrorNotice, Field, Loading, StatusBadge } from "../components/ui";
import { useLoad, useOnline } from "../hooks";
import { can } from "../session";

export function CommandPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const online = useOnline();
  const catalog = useLoad((s) => api.get<Catalog>(`/orgs/${org}/providers`, s), [org]);
  const projects = useLoad((s) => api.get<Paged<Project>>(`/orgs/${org}/projects?limit=100`, s), [org]);
  const [provider, setProvider] = useState<ProviderId>("anthropic");
  const [task, setTask] = useState<Task>("planning");
  const [mode, setMode] = useState<Mode>("fixture");
  const [project, setProject] = useState("");
  const [prompt, setPrompt] = useState("");
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<RunResult | null>(null);
  const abort = useRef<AbortController | null>(null);

  if (catalog.loading && !catalog.data) return <Loading />;
  if (catalog.error || !catalog.data) return <ErrorNotice message={catalog.error ?? "Unavailable"} onRetry={catalog.reload} />;
  const c = catalog.data;
  const selected = c.providers.find((p) => p.id === provider);
  const liveAvailable = selected?.status === "configured";
  const effectiveMode: Mode = liveAvailable ? mode : "fixture";
  const max = c.prompt_max_chars;

  async function run(e: FormEvent) {
    e.preventDefault();
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const r = await api.post<RunResult>(
        `/orgs/${org}/provider-runs`,
        {
          provider,
          task,
          prompt,
          mode: effectiveMode,
          project_id: project || null,
          acknowledge_external_transfer: effectiveMode === "live" ? ack : false,
        },
        controller.signal,
      );
      setResult(r);
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") setError("Request cancelled.");
      else setError(errorMessage(err));
    } finally {
      setBusy(false);
      abort.current = null;
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Command</span>
          <h1>Ask one provider</h1>
          <p>One explicit, text-only request. No tools, no agents, no automatic actions.</p>
        </div>
      </div>

      <form className="stack" onSubmit={run} aria-label="Provider request">
        <fieldset className="card stack">
          <legend>Provider</legend>
          <div className="choice">
            {c.providers.map((p) => (
              <label key={p.id}>
                <input type="radio" name="provider" value={p.id} checked={provider === p.id} onChange={() => setProvider(p.id)} />
                <strong>{p.vendor}</strong>
                <span className="small muted">
                  {p.perspective} · {p.model ?? "no live model"}
                </span>
                <StatusBadge
                  status={p.status}
                  label={p.status === "configured" ? "Live available · unverified" : "Fixture only"}
                />
              </label>
            ))}
          </div>
        </fieldset>

        <section className="card stack">
          <div className="grid grid-3">
            <Field label="Task">
              {(p) => (
                <select {...p} value={task} onChange={(e) => setTask(e.target.value as Task)}>
                  {c.tasks.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.label}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <Field label="Project (optional)" hint="Links the usage record to a project.">
              {(p) => (
                <select {...p} value={project} onChange={(e) => setProject(e.target.value)}>
                  <option value="">None</option>
                  {projects.data?.items.map((x) => (
                    <option key={x.id} value={x.id}>
                      {x.name}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <fieldset>
              <legend>Mode</legend>
              <div className="segmented">
                <label>
                  <input type="radio" name="mode" value="fixture" checked={effectiveMode === "fixture"} onChange={() => setMode("fixture")} />
                  Fixture
                </label>
                <label aria-disabled={!liveAvailable}>
                  <input
                    type="radio"
                    name="mode"
                    value="live"
                    disabled={!liveAvailable}
                    checked={effectiveMode === "live"}
                    onChange={() => setMode("live")}
                  />
                  Live
                </label>
              </div>
            </fieldset>
          </div>
          <p className="small muted">{c.tasks.find((t) => t.id === task)?.instruction}</p>

          <Field
            label="Brief"
            hint={`${prompt.length.toLocaleString()} / ${max.toLocaleString()} characters. Never include passwords, keys or personal data you are not authorized to share.`}
          >
            {(p) => <textarea {...p} required maxLength={max} value={prompt} onChange={(e) => setPrompt(e.target.value)} />}
          </Field>

          {effectiveMode === "live" && selected ? (
            <div className="notice warn stack" role="note">
              <strong>This brief will leave QUORUM.</strong>
              <span>{selected.data_handling}</span>
              <label className="check">
                <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
                I am authorized to send this brief to {selected.vendor} ({selected.model}) for this purpose. Charges may apply.
              </label>
            </div>
          ) : (
            <div className="notice" role="note">
              Fixture mode returns pre-written illustrative text. Nothing is sent to any AI provider.
            </div>
          )}

          <div className="row">
            <button
              type="submit"
              className="btn primary"
              disabled={busy || !online || !prompt.trim() || (effectiveMode === "live" && !ack)}
            >
              {busy ? "Waiting for response…" : effectiveMode === "live" ? `Send to ${selected?.vendor}` : "Show fixture"}
            </button>
            {busy && (
              <button type="button" className="btn" onClick={() => abort.current?.abort()}>
                Cancel
              </button>
            )}
            {!online && <span className="small muted">Requires an internet connection.</span>}
          </div>
        </section>
      </form>

      <div aria-live="polite">
        {busy && <Loading label="Waiting for the provider. You can cancel at any time." />}
        {error && <ErrorNotice message={error} />}
        {result && <Result org={org} m={m} result={result} projects={projects.data?.items ?? []} defaultProject={project} />}
      </div>
    </>
  );
}

function Result({
  org,
  m,
  result,
  projects,
  defaultProject,
}: {
  org: string;
  m: Membership;
  result: RunResult;
  projects: Project[];
  defaultProject: string;
}) {
  const [target, setTarget] = useState(defaultProject);
  const [title, setTitle] = useState("Command output");
  const [saved, setSaved] = useState<DocumentDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setError(null);
    try {
      const d = await api.post<DocumentDetail>(`/orgs/${org}/projects/${target}/documents`, {
        title,
        category: "general",
        body: result.text,
        ai_assisted: result.mode === "live",
      });
      setSaved(d);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section className={`card stack result ${result.mode}`} aria-labelledby="result-h">
      <div className="row-between">
        <h2 id="result-h">
          {result.mode === "live" ? "AI response" : "Illustrative fixture"} · {result.provider}
        </h2>
        <span className="badge plain">{result.mode === "live" ? `LIVE · ${result.model}` : "FIXTURE"}</span>
      </div>
      <p className="small muted">{result.notice}</p>
      <div className="prose">{result.text}</div>
      {result.truncated && <p className="small muted">Response truncated to the display limit.</p>}
      <p className="small muted">
        {result.usage
          ? `Tokens reported by provider: ${result.usage.input_tokens ?? "?"} in / ${result.usage.output_tokens ?? "?"} out · ${result.latency_ms} ms`
          : "No token usage (fixture)."}
      </p>
      {can(m, "documents_write") && (
        <div className="stack">
          {error && <ErrorNotice message={error} />}
          {saved ? (
            <div className="notice ok" role="status">
              Saved as a draft document. <Link to={`/o/${org}/documents/${saved.id}`}>Open it for review</Link>
            </div>
          ) : (
            <div className="grid grid-3">
              <Field label="Save to project">
                {(p) => (
                  <select {...p} value={target} onChange={(e) => setTarget(e.target.value)}>
                    <option value="">Choose…</option>
                    {projects.map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.name}
                      </option>
                    ))}
                  </select>
                )}
              </Field>
              <Field label="Document title">
                {(p) => <input {...p} type="text" value={title} onChange={(e) => setTitle(e.target.value)} />}
              </Field>
              <div className="field">
                <span className="label" aria-hidden="true">
                  &nbsp;
                </span>
                <button type="button" className="btn" disabled={!target || !title.trim()} onClick={() => void save()}>
                  Save as draft document
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
