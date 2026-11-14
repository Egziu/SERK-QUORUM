import { useState, type FormEvent } from "react";
import { Link, useNavigate, useOutletContext, useSearchParams } from "react-router";
import { ApiError, api, errorMessage } from "../api/client";
import type { Membership, Paged, Project } from "../api/types";
import { Empty, ErrorNotice, Field, Loading, StatusBadge } from "../components/ui";
import { formatDate, plural, useLoad } from "../hooks";
import { can } from "../session";

export function ProjectsPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const [params, setParams] = useSearchParams();
  const showArchived = params.get("archived") === "1";
  const [creating, setCreating] = useState(params.get("new") === "1");
  const [query, setQuery] = useState("");
  const projects = useLoad(
    (s) => api.get<Paged<Project>>(`/orgs/${org}/projects?limit=100&include_archived=${showArchived}`, s),
    [org, showArchived],
  );
  const items = (projects.data?.items ?? []).filter((p) => p.name.toLowerCase().includes(query.trim().toLowerCase()));

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Projects</span>
          <h1>Projects</h1>
          <p>Each project holds its documents and decision register.</p>
        </div>
        {can(m, "projects_manage") && !creating && (
          <button className="btn primary" type="button" onClick={() => setCreating(true)}>
            New project
          </button>
        )}
      </div>

      {creating && <NewProject org={org} onCancel={() => setCreating(false)} />}

      <section className="card">
        <div className="row-between">
          <div className="field grow">
            <label htmlFor="project-filter" className="visually-hidden">
              Filter projects
            </label>
            <input
              id="project-filter"
              type="search"
              placeholder="Filter by name"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <label className="check">
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(e) => setParams(e.target.checked ? { archived: "1" } : {})}
            />
            Show archived
          </label>
        </div>
        <div className="mt-3">
          {projects.loading ? (
            <Loading />
          ) : projects.error ? (
            <ErrorNotice message={projects.error} onRetry={projects.reload} />
          ) : items.length === 0 ? (
            <Empty title={query ? "No matching projects" : "No projects yet"}>
              {can(m, "projects_manage") && !query && <span>Create a project to start a document library and decision register.</span>}
            </Empty>
          ) : (
            <ul className="list">
              {items.map((p) => (
                <li key={p.id}>
                  <Link className="list-link" to={p.id}>
                    <span>
                      <span className="title">{p.name}</span>
                      <br />
                      <span className="small muted">
                        {p.description ? `${p.description.slice(0, 120)}${p.description.length > 120 ? "…" : ""} · ` : ""}
                        {plural(p.document_count, "document")} · {plural(p.decision_count, "decision")} · updated {formatDate(p.updated_at)}
                      </span>
                    </span>
                    <StatusBadge status={p.status} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>
    </>
  );
}

function NewProject({ org, onCancel }: { org: string; onCancel: () => void }) {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const p = await api.post<Project>(`/orgs/${org}/projects`, { name, description });
      navigate(p.id);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError(0, "unknown", errorMessage(err)));
      setBusy(false);
    }
  }

  return (
    <section className="card" aria-labelledby="new-project-h">
      <h2 id="new-project-h">New project</h2>
      <form className="stack mt-3" onSubmit={submit}>
        {error && !error.field && <ErrorNotice message={error.message} />}
        <Field label="Name" error={error?.field === "name" ? error.message : null}>
          {(p) => <input {...p} type="text" required maxLength={120} value={name} onChange={(e) => setName(e.target.value)} />}
        </Field>
        <Field label="Description" hint="Objectives, scope and completion criteria." error={error?.field === "description" ? error.message : null}>
          {(p) => <textarea {...p} maxLength={4000} value={description} onChange={(e) => setDescription(e.target.value)} />}
        </Field>
        <div className="row">
          <button className="btn primary" type="submit" disabled={busy || !name.trim()}>
            {busy ? "Creating…" : "Create project"}
          </button>
          <button className="btn ghost" type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </section>
  );
}
