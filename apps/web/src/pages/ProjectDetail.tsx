import { useState, type FormEvent } from "react";
import { Link, useNavigate, useOutletContext, useParams, useSearchParams } from "react-router";
import { ApiError, api, errorMessage } from "../api/client";
import {
  DOCUMENT_CATEGORIES,
  type Decision,
  type DocumentCategory,
  type DocumentDetail,
  type DocumentSummary,
  type Membership,
  type Paged,
  type Project,
  type ProjectStatus,
} from "../api/types";
import { Empty, ErrorNotice, Field, Loading, StatusBadge, TableWrap } from "../components/ui";
import { formatDate, useLoad } from "../hooks";
import { can } from "../session";

type Tab = "documents" | "decisions" | "settings";

export function ProjectDetailPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const { project: projectId } = useParams();
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") as Tab | null) ?? "documents";
  const project = useLoad((s) => api.get<Project>(`/orgs/${org}/projects/${projectId}`, s), [org, projectId]);

  if (project.loading && !project.data) return <Loading />;
  if (project.error || !project.data) return <ErrorNotice message={project.error ?? "Project not found."} />;
  const p = project.data;
  const archived = p.status === "archived";
  const tabs: [Tab, string][] = [
    ["documents", `Documents (${p.document_count})`],
    ["decisions", `Decisions (${p.decision_count})`],
    ...(can(m, "projects_manage") ? ([["settings", "Settings"]] as [Tab, string][]) : []),
  ];

  return (
    <>
      <div className="page-head">
        <div>
          <Link to=".." relative="path" className="eyebrow">
            ← Projects
          </Link>
          <h1>{p.name}</h1>
          {p.description && <p className="prose">{p.description}</p>}
        </div>
        <StatusBadge status={p.status} />
      </div>
      {archived && (
        <div className="notice warn" role="status">
          This project is archived and read-only. {can(m, "projects_manage") ? "Reactivate it in Settings to make changes." : ""}
        </div>
      )}
      <div
        className="tabs"
        role="tablist"
        aria-label="Project sections"
        onKeyDown={(e) => {
          if (e.key !== "ArrowRight" && e.key !== "ArrowLeft" && e.key !== "Home" && e.key !== "End") return;
          e.preventDefault();
          const i = tabs.findIndex(([id]) => id === tab);
          const n = tabs.length;
          const next =
            e.key === "Home" ? 0 : e.key === "End" ? n - 1 : (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
          const id = tabs[next]?.[0];
          if (id) {
            setParams({ tab: id });
            document.getElementById(`tab-${id}`)?.focus();
          }
        }}
      >
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`panel-${id}`}
            tabIndex={tab === id ? 0 : -1}
            onClick={() => setParams({ tab: id })}
          >
            {label}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === "documents" && <Documents m={m} project={p} onChange={project.reload} />}
        {tab === "decisions" && <Decisions m={m} project={p} onChange={project.reload} />}
        {tab === "settings" && <Settings org={org} project={p} onSaved={project.reload} />}
      </div>
    </>
  );
}

function Documents({ m, project, onChange }: { m: Membership; project: Project; onChange: () => void }) {
  const org = m.organization_id;
  const navigate = useNavigate();
  const docs = useLoad(
    (s) => api.get<Paged<DocumentSummary>>(`/orgs/${org}/projects/${project.id}/documents?limit=100`, s),
    [org, project.id],
  );
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState<DocumentCategory>("project_plan");
  const [error, setError] = useState<string | null>(null);
  const writable = can(m, "documents_write") && project.status !== "archived";

  async function create(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const d = await api.post<DocumentDetail>(`/orgs/${org}/projects/${project.id}/documents`, { title, category, body: "" });
      onChange();
      navigate(`../../documents/${d.id}`, { relative: "path" });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section className="card stack">
      <div className="row-between">
        <h2>Document library</h2>
        {writable && !creating && (
          <button type="button" className="btn primary" onClick={() => setCreating(true)}>
            New document
          </button>
        )}
      </div>
      {creating && (
        <form className="stack card" onSubmit={create} aria-label="New document">
          {error && <ErrorNotice message={error} />}
          <div className="grid grid-2">
            <Field label="Title">
              {(p) => <input {...p} type="text" required maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} />}
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
          <div className="row">
            <button className="btn primary" type="submit" disabled={!title.trim()}>
              Create and edit
            </button>
            <button className="btn ghost" type="button" onClick={() => setCreating(false)}>
              Cancel
            </button>
          </div>
        </form>
      )}
      {docs.loading ? (
        <Loading />
      ) : docs.error ? (
        <ErrorNotice message={docs.error} onRetry={docs.reload} />
      ) : docs.data?.items.length === 0 ? (
        <Empty title="No documents yet">{writable && <span>Requirements, plans, reports and minutes live here with full version history.</span>}</Empty>
      ) : (
        <ul className="list">
          {docs.data?.items.map((d) => (
            <li key={d.id}>
              <Link className="list-link" to={`../../documents/${d.id}`} relative="path">
                <span>
                  <span className="title">{d.title}</span>
                  <br />
                  <span className="small muted">
                    {DOCUMENT_CATEGORIES[d.category]} · v{d.current_version} · updated {formatDate(d.updated_at)}
                  </span>
                </span>
                <StatusBadge status={d.status} />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Decisions({ m, project, onChange }: { m: Membership; project: Project; onChange: () => void }) {
  const org = m.organization_id;
  const navigate = useNavigate();
  const list = useLoad(
    (s) => api.get<Paged<Decision>>(`/orgs/${org}/projects/${project.id}/decisions?limit=100`, s),
    [org, project.id],
  );
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [problem, setProblem] = useState("");
  const [error, setError] = useState<string | null>(null);
  const writable = can(m, "decisions_write") && project.status !== "archived";

  async function create(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const d = await api.post<Decision>(`/orgs/${org}/projects/${project.id}/decisions`, {
        title,
        problem_statement: problem,
      });
      onChange();
      navigate(`../../decisions/${d.id}`, { relative: "path" });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section className="card stack">
      <div className="row-between">
        <h2>Decision register</h2>
        {writable && !creating && (
          <button type="button" className="btn primary" onClick={() => setCreating(true)}>
            Record a decision
          </button>
        )}
      </div>
      {creating && (
        <form className="stack card" onSubmit={create} aria-label="New decision">
          {error && <ErrorNotice message={error} />}
          <Field label="Title">
            {(p) => <input {...p} type="text" required maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} />}
          </Field>
          <Field label="Problem statement" hint="What needs deciding, and why now?">
            {(p) => <textarea {...p} required value={problem} onChange={(e) => setProblem(e.target.value)} />}
          </Field>
          <div className="row">
            <button className="btn primary" type="submit" disabled={!title.trim() || !problem.trim()}>
              Create draft
            </button>
            <button className="btn ghost" type="button" onClick={() => setCreating(false)}>
              Cancel
            </button>
          </div>
        </form>
      )}
      {list.loading ? (
        <Loading />
      ) : list.error ? (
        <ErrorNotice message={list.error} onRetry={list.reload} />
      ) : list.data?.items.length === 0 ? (
        <Empty title="No decisions recorded">
          <span>Record what was decided, the alternatives considered and who is accountable.</span>
        </Empty>
      ) : (
        <TableWrap label="Decision register">
          <table>
            <thead>
              <tr>
                <th scope="col">Ref</th>
                <th scope="col">Decision</th>
                <th scope="col">Status</th>
                <th scope="col">Decision maker</th>
                <th scope="col">Updated</th>
              </tr>
            </thead>
            <tbody>
              {list.data?.items.map((d) => (
                <tr key={d.id}>
                  <td>
                    <code>{d.reference}</code>
                  </td>
                  <td>
                    <Link to={`../../decisions/${d.id}`} relative="path">
                      {d.title}
                    </Link>
                  </td>
                  <td>
                    <StatusBadge status={d.status} />
                  </td>
                  <td>{d.decision_maker_name ?? <span className="muted">Not assigned</span>}</td>
                  <td className="small muted">{formatDate(d.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableWrap>
      )}
    </section>
  );
}

function Settings({ org, project, onSaved }: { org: string; project: Project; onSaved: () => void }) {
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description);
  const [status, setStatus] = useState<ProjectStatus>(project.status);
  const [error, setError] = useState<ApiError | null>(null);
  const [saved, setSaved] = useState(false);

  async function save(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(false);
    try {
      await api.patch(`/orgs/${org}/projects/${project.id}`, { name, description, status });
      setSaved(true);
      onSaved();
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError(0, "unknown", errorMessage(err)));
    }
  }

  return (
    <section className="card">
      <form className="stack" onSubmit={save}>
        <h2>Project settings</h2>
        {error && !error.field && <ErrorNotice message={error.message} />}
        {saved && (
          <div className="notice ok" role="status">
            Saved.
          </div>
        )}
        <Field label="Name" error={error?.field === "name" ? error.message : null}>
          {(p) => <input {...p} type="text" maxLength={120} value={name} onChange={(e) => setName(e.target.value)} />}
        </Field>
        <Field label="Description">
          {(p) => <textarea {...p} maxLength={4000} value={description} onChange={(e) => setDescription(e.target.value)} />}
        </Field>
        <Field label="Status" hint="Archived projects are read-only and hidden from the default project list.">
          {(p) => (
            <select {...p} value={status} onChange={(e) => setStatus(e.target.value as ProjectStatus)}>
              <option value="active">Active</option>
              <option value="on_hold">On hold</option>
              <option value="completed">Completed</option>
              <option value="archived">Archived</option>
            </select>
          )}
        </Field>
        <div>
          <button className="btn primary" type="submit">
            Save changes
          </button>
        </div>
      </form>
    </section>
  );
}
