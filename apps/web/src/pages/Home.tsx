import { Link, useOutletContext } from "react-router";
import { api } from "../api/client";
import type { Catalog, Membership, Paged, Project } from "../api/types";
import { ErrorNotice, Loading, StatusBadge } from "../components/ui";
import { formatDate, plural, useLoad } from "../hooks";
import { can } from "../session";

export function HomePage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const projects = useLoad((s) => api.get<Paged<Project>>(`/orgs/${org}/projects?limit=100`, s), [org]);
  const catalog = useLoad((s) => api.get<Catalog>(`/orgs/${org}/providers`, s), [org]);

  const items = projects.data?.items ?? [];
  const totals = items.reduce(
    (acc, p) => ({ docs: acc.docs + p.document_count, decisions: acc.decisions + p.decision_count }),
    { docs: 0, decisions: 0 },
  );

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Home</span>
          <h1>{m.organization_name}</h1>
          <p>Projects, documents and decisions — owned by people, informed by AI.</p>
        </div>
        <div className="row">
          {can(m, "projects_manage") && (
            <Link className="btn" to="projects?new=1">
              New project
            </Link>
          )}
          {can(m, "providers_use") && (
            <Link className="btn primary" to="command">
              Open Command
            </Link>
          )}
        </div>
      </div>

      {projects.error && <ErrorNotice message={projects.error} onRetry={projects.reload} />}
      <section className="grid grid-3" aria-label="Summary">
        <div className="card stat">
          <strong>{projects.loading ? "–" : items.length}</strong>
          <span>Active projects{projects.data?.has_more ? " (first 100)" : ""}</span>
        </div>
        <div className="card stat">
          <strong>{projects.loading ? "–" : totals.docs}</strong>
          <span>Documents</span>
        </div>
        <div className="card stat">
          <strong>{projects.loading ? "–" : totals.decisions}</strong>
          <span>Decision records</span>
        </div>
      </section>
      <p className="small muted">Counts are live database totals for active projects in this organization.</p>

      <div className="split">
        <section className="card" aria-labelledby="recent-projects">
          <div className="card-head">
            <h2 id="recent-projects">Recently updated projects</h2>
            <Link to="projects" className="small">
              All projects
            </Link>
          </div>
          {projects.loading ? (
            <Loading />
          ) : items.length === 0 ? (
            <p className="muted">No projects yet.</p>
          ) : (
            <ul className="list">
              {items.slice(0, 6).map((p) => (
                <li key={p.id}>
                  <Link className="list-link" to={`projects/${p.id}`}>
                    <span>
                      <span className="title">{p.name}</span>
                      <br />
                      <span className="small muted">
                        {plural(p.document_count, "document")} · {plural(p.decision_count, "decision")} · updated {formatDate(p.updated_at)}
                      </span>
                    </span>
                    <StatusBadge status={p.status} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="card" aria-labelledby="providers-h">
          <div className="card-head">
            <h2 id="providers-h">AI providers</h2>
          </div>
          {catalog.loading ? (
            <Loading />
          ) : catalog.error ? (
            <ErrorNotice message={catalog.error} />
          ) : (
            <ul className="list">
              {catalog.data?.providers.map((p) => (
                <li key={p.id} className="row-between pad-y">
                  <span>
                    <strong>{p.vendor}</strong> <span className="small muted">· {p.perspective}</span>
                  </span>
                  <StatusBadge
                    status={p.status}
                    label={p.status === "disabled" ? "Fixtures only" : p.status === "configured" ? "Configured · unverified" : "Not configured"}
                  />
                </li>
              ))}
            </ul>
          )}
          <p className="small muted mt-3">
            “Configured” means a key and model are set on the server. QUORUM does not claim a connection is verified.
          </p>
        </section>
      </div>
    </>
  );
}
