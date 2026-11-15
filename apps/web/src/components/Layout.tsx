import { useEffect, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate, useParams } from "react-router";
import { ROLE_LABELS } from "../api/types";
import { useOnline } from "../hooks";
import { can, useMembership, useSession } from "../session";
import { ThemeToggle } from "./ThemeToggle";

export function Layout() {
  const { org } = useParams();
  const { session, signOut } = useSession();
  const membership = useMembership(org);
  const online = useOnline();
  const navigate = useNavigate();
  const location = useLocation();
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => setMenuOpen(false), [location.pathname]);
  useEffect(() => {
    if (!menuOpen) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenuOpen(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menuOpen]);

  if (!session) return null;
  const base = `/o/${org}`;

  // Navigation visibility follows the role for convenience only; every
  // action is authorized again by the server.
  return (
    <div className="shell">
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <aside className="sidebar" data-open={menuOpen} aria-label="Primary">
        <NavLink to={base} className="brand" end>
          <img src="/logo.svg" alt="" />
          <span>
            <strong>QUORUM</strong>
            <small>SERK TECHNOLOGIES</small>
          </span>
        </NavLink>

        <div className="field">
          <label htmlFor="org-switch" className="nav-label">
            Organization
          </label>
          <select
            id="org-switch"
            value={org}
            onChange={(e) => navigate(e.target.value === "__new" ? "/organizations/new" : `/o/${e.target.value}`)}
          >
            {session.memberships.map((m) => (
              <option key={m.organization_id} value={m.organization_id}>
                {m.organization_name}
              </option>
            ))}
            <option value="__new">+ New organization</option>
          </select>
        </div>

        <nav className="nav" aria-label="Workspace">
          <span className="nav-label">Workspace</span>
          <NavLink to={base} end>
            Home
          </NavLink>
          {can(membership, "providers_use") && <NavLink to={`${base}/connect`}>Connect</NavLink>}
          {can(membership, "providers_use") && <NavLink to={`${base}/command`}>Command</NavLink>}
          <NavLink to={`${base}/projects`}>Projects</NavLink>
        </nav>

        <nav className="nav" aria-label="Organization">
          <span className="nav-label">Organization</span>
          <NavLink to={`${base}/members`}>Members</NavLink>
          {can(membership, "usage_read") && <NavLink to={`${base}/providers`}>Providers &amp; usage</NavLink>}
          {can(membership, "audit_read") && <NavLink to={`${base}/audit`}>Audit records</NavLink>}
        </nav>

        <div className="sidebar-footer">
          <span>Intelligence. In concert.</span>
          <span className="small">v0.3 · engineering preview</span>
        </div>
      </aside>

      <div className="main">
        <header className="topbar">
          <div className="row">
            <button
              type="button"
              className="btn small menu-button"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen((o) => !o)}
            >
              Menu
            </button>
            <span className="muted small">
              {membership ? `${membership.organization_name} · ${ROLE_LABELS[membership.role]}` : ""}
            </span>
          </div>
          <div className="row">
            <ThemeToggle />
            <span className="small" title={session.user.email}>
              {session.user.display_name}
            </span>
            <button type="button" className="btn small" onClick={() => void signOut().then(() => navigate("/login"))}>
              Sign out
            </button>
          </div>
        </header>
        {!online && (
          <div className="offline-banner" role="status">
            You are offline. Changes cannot be saved until the connection returns.
          </div>
        )}
        <main id="main" className="content" tabIndex={-1}>
          {membership ? (
            <Outlet context={membership} />
          ) : (
            <div className="notice error" role="alert">
              Organization not found or you are not a member.
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
