import { useEffect, useState, type FormEvent } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { api, errorMessage } from "../api/client";
import {
  DOCUMENT_CATEGORIES,
  type DocumentCategory,
  type DocumentDetail,
  type DocumentVersion,
  type Membership,
  type VersionSummary,
} from "../api/types";
import { ErrorNotice, Field, Loading, StatusBadge } from "../components/ui";
import { formatDate, useLoad } from "../hooks";
import { can } from "../session";

export function DocumentPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const { document: id } = useParams();
  const doc = useLoad((s) => api.get<DocumentDetail>(`/orgs/${org}/documents/${id}`, s), [org, id]);
  const versions = useLoad((s) => api.get<VersionSummary[]>(`/orgs/${org}/documents/${id}/versions`, s), [org, id]);
  const [viewing, setViewing] = useState<DocumentVersion | null>(null);

  if (doc.loading && !doc.data) return <Loading />;
  if (doc.error || !doc.data) return <ErrorNotice message={doc.error ?? "Document not found."} onRetry={doc.reload} />;
  const d = doc.data;
  const reloadAll = () => {
    doc.reload();
    versions.reload();
    setViewing(null);
  };

  return (
    <>
      <div className="page-head">
        <div>
          <Link to={`/o/${org}/projects/${d.project_id}`} className="eyebrow">
            ← Project
          </Link>
          <h1>{d.title}</h1>
          <p>
            {DOCUMENT_CATEGORIES[d.category]} · version {d.current_version}
            {d.approved_version ? ` · approved version ${d.approved_version}` : ""}
          </p>
        </div>
        <div className="row">
          <StatusBadge status={d.status} />
          <a className="btn" href={`/api/v1/orgs/${org}/documents/${d.id}/export.md`} download>
            Export Markdown
          </a>
        </div>
      </div>

      <div className="split">
        <div className="stack">
          {viewing ? (
            <section className="card stack" aria-labelledby="old-version">
              <div className="row-between">
                <h2 id="old-version">Version {viewing.version} (read-only)</h2>
                <button className="btn small" type="button" onClick={() => setViewing(null)}>
                  Back to current
                </button>
              </div>
              <p className="small muted">
                Saved by {viewing.created_by_name} on {formatDate(viewing.created_at)}
                {viewing.change_note ? ` — “${viewing.change_note}”` : ""}
              </p>
              {viewing.ai_assisted && <AiNotice />}
              <h3>{viewing.title}</h3>
              <div className="prose">{viewing.body || <span className="muted">Empty</span>}</div>
            </section>
          ) : can(m, "documents_write") ? (
            <Editor key={d.current_version} org={org} doc={d} onSaved={reloadAll} />
          ) : (
            <section className="card stack">
              {d.current.ai_assisted && <AiNotice />}
              <div className="prose">{d.current.body || <span className="muted">This document is empty.</span>}</div>
            </section>
          )}
        </div>

        <aside className="stack" aria-label="Document details">
          {can(m, "documents_approve") && d.status !== "approved" && (
            <Approve org={org} doc={d} onApproved={reloadAll} />
          )}
          <section className="card stack" aria-labelledby="history-h">
            <h2 id="history-h">Version history</h2>
            {versions.loading ? (
              <Loading />
            ) : versions.error ? (
              <ErrorNotice message={versions.error} />
            ) : (
              <ol className="timeline">
                {versions.data?.map((v) => (
                  <li key={v.version}>
                    <div className="row-between">
                      <strong>v{v.version}</strong>
                      <span className="row">
                        {v.version === d.approved_version && <StatusBadge status="approved" />}
                        {v.ai_assisted && <span className="badge plain">AI-assisted</span>}
                      </span>
                    </div>
                    <div className="small muted">
                      {v.created_by_name} · {formatDate(v.created_at)}
                    </div>
                    {v.change_note && <div className="small">{v.change_note}</div>}
                    {v.version !== d.current_version && (
                      <button
                        type="button"
                        className="btn small ghost"
                        onClick={() =>
                          void api
                            .get<DocumentVersion>(`/orgs/${org}/documents/${d.id}/versions/${v.version}`)
                            .then(setViewing)
                        }
                      >
                        View v{v.version}
                      </button>
                    )}
                  </li>
                ))}
              </ol>
            )}
          </section>
        </aside>
      </div>
    </>
  );
}

function AiNotice() {
  return (
    <div className="notice warn" role="note">
      Contains AI-assisted content. Verify facts, figures and citations before relying on it.
    </div>
  );
}

const DRAFT_KEY = (id: string) => `quorum.draft.${id}`;

/** Saves a new immutable version. An unsaved draft can optionally be kept
 *  in this browser's session storage (never for other tabs or devices). */
function Editor({ org, doc, onSaved }: { org: string; doc: DocumentDetail; onSaved: () => void }) {
  const [title, setTitle] = useState(doc.current.title);
  const [category, setCategory] = useState<DocumentCategory>(doc.category);
  const [body, setBody] = useState(doc.current.body);
  const [note, setNote] = useState("");
  const [aiAssisted, setAiAssisted] = useState(doc.current.ai_assisted);
  const [keepDraft, setKeepDraft] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dirty = title !== doc.current.title || body !== doc.current.body || category !== doc.category;

  // Restore a draft kept in this tab, if the user opted in earlier.
  useEffect(() => {
    try {
      const raw = sessionStorage.getItem(DRAFT_KEY(doc.id));
      if (!raw) return;
      const saved = JSON.parse(raw) as { base: number; title: string; body: string };
      if (saved.base === doc.current_version) {
        setTitle(saved.title);
        setBody(saved.body);
        setKeepDraft(true);
      }
    } catch {
      /* ignore */
    }
  }, [doc.id, doc.current_version]);

  useEffect(() => {
    try {
      if (keepDraft && dirty) {
        sessionStorage.setItem(DRAFT_KEY(doc.id), JSON.stringify({ base: doc.current_version, title, body }));
      } else {
        sessionStorage.removeItem(DRAFT_KEY(doc.id));
      }
    } catch {
      /* ignore */
    }
  }, [keepDraft, dirty, title, body, doc.id, doc.current_version]);

  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.post(`/orgs/${org}/documents/${doc.id}/versions`, {
        base_version: doc.current_version,
        title,
        category,
        body,
        ai_assisted: aiAssisted,
        change_note: note,
      });
      try {
        sessionStorage.removeItem(DRAFT_KEY(doc.id));
      } catch {
        /* ignore */
      }
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  }

  return (
    <form className="card stack" onSubmit={save} aria-label="Edit document">
      {error && <ErrorNotice message={error} />}
      {doc.status === "approved" && (
        <div className="notice" role="note">
          The current version is approved. Saving creates a new draft version; the approved version stays in history.
        </div>
      )}
      <div className="grid grid-2">
        <Field label="Title">
          {(p) => <input {...p} type="text" maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} />}
        </Field>
        <Field label="Category">
          {(p) => (
            <select {...p} value={category} onChange={(e) => setCategory(e.target.value as DocumentCategory)}>
              {Object.entries(DOCUMENT_CATEGORIES).map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </select>
          )}
        </Field>
      </div>
      <Field label="Content" hint="Markdown is supported in exports. Do not paste credentials or secrets.">
        {(p) => <textarea {...p} className="editor" value={body} onChange={(e) => setBody(e.target.value)} />}
      </Field>
      <Field label="Change note" hint="Optional summary shown in the version history.">
        {(p) => <input {...p} type="text" maxLength={500} value={note} onChange={(e) => setNote(e.target.value)} />}
      </Field>
      <label className="check">
        <input type="checkbox" checked={aiAssisted} onChange={(e) => setAiAssisted(e.target.checked)} />
        This version contains AI-assisted content
      </label>
      <label className="check">
        <input type="checkbox" checked={keepDraft} onChange={(e) => setKeepDraft(e.target.checked)} />
        Keep my unsaved draft in this browser tab if I lose connection (cleared when the tab closes)
      </label>
      <div className="row">
        <button className="btn primary" type="submit" disabled={busy || !title.trim() || !dirty}>
          {busy ? "Saving…" : `Save as version ${doc.current_version + 1}`}
        </button>
        {!dirty && <span className="small muted">No changes yet.</span>}
      </div>
    </form>
  );
}

function Approve({ org, doc, onApproved }: { org: string; doc: DocumentDetail; onApproved: () => void }) {
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function approve() {
    setError(null);
    try {
      await api.post(`/orgs/${org}/documents/${doc.id}/approve`, { version: doc.current_version });
      onApproved();
    } catch (err) {
      setError(errorMessage(err));
    }
  }
  return (
    <section className="card stack" aria-labelledby="approve-h">
      <h2 id="approve-h">Approval</h2>
      {error && <ErrorNotice message={error} />}
      <label className="check">
        <input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
        I have reviewed version {doc.current_version} and approve it.
      </label>
      <button type="button" className="btn primary" disabled={!confirmed} onClick={() => void approve()}>
        Approve version {doc.current_version}
      </button>
    </section>
  );
}
