import { useState, type FormEvent } from "react";
import { useNavigate, useOutletContext, useSearchParams } from "react-router";
import { api, errorMessage } from "../api/client";
import {
  ROLE_LABELS,
  type AuditEvent,
  type Catalog,
  type Invitation,
  type Member,
  type Membership,
  type Paged,
  type Role,
  type SessionView,
  type UsageRecord,
} from "../api/types";
import { Empty, ErrorNotice, Field, Loading, StatusBadge, TableWrap, humanize } from "../components/ui";
import { formatDate, useLoad } from "../hooks";
import { can, useSession } from "../session";

const ROLES = Object.keys(ROLE_LABELS) as Role[];

export function MembersPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const { session } = useSession();
  const manage = can(m, "members_manage");
  const members = useLoad((s) => api.get<Member[]>(`/orgs/${org}/members`, s), [org]);
  const invitations = useLoad(
    (s) => (manage ? api.get<Invitation[]>(`/orgs/${org}/invitations`, s) : Promise.resolve([])),
    [org, manage],
  );
  const [error, setError] = useState<string | null>(null);
  const assignable = ROLES.filter((r) => m.role === "owner" || r !== "owner");

  async function act(fn: () => Promise<unknown>) {
    setError(null);
    try {
      await fn();
      members.reload();
      invitations.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Organization</span>
          <h1>Members</h1>
          <p>Roles are enforced on the server for every request.</p>
        </div>
      </div>
      {error && <ErrorNotice message={error} />}
      {manage && <Invite org={org} roles={assignable} onDone={invitations.reload} />}
      <section className="card">
        {members.loading ? (
          <Loading />
        ) : members.error ? (
          <ErrorNotice message={members.error} />
        ) : (
          <TableWrap label="Members">
            <table>
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">Email</th>
                  <th scope="col">Role</th>
                  <th scope="col">Joined</th>
                  {manage && <th scope="col">
                    <span className="visually-hidden">Actions</span>
                  </th>}
                </tr>
              </thead>
              <tbody>
                {members.data?.map((x) => {
                  const editable = manage && (m.role === "owner" || x.role !== "owner");
                  return (
                    <tr key={x.user_id}>
                      <td>
                        {x.display_name}
                        {x.user_id === session?.user.id && <span className="muted"> (you)</span>}
                      </td>
                      <td>{x.email}</td>
                      <td>
                        {editable ? (
                          <select
                            aria-label={`Role for ${x.display_name}`}
                            value={x.role}
                            onChange={(e) =>
                              void act(() => api.patch(`/orgs/${org}/members/${x.user_id}`, { role: e.target.value }))
                            }
                          >
                            {assignable.map((r) => (
                              <option key={r} value={r}>
                                {ROLE_LABELS[r]}
                              </option>
                            ))}
                          </select>
                        ) : (
                          ROLE_LABELS[x.role]
                        )}
                      </td>
                      <td className="small muted">{formatDate(x.joined_at)}</td>
                      {manage && (
                        <td>
                          {editable && x.user_id !== session?.user.id && (
                            <button
                              type="button"
                              className="btn small danger"
                              onClick={() => {
                                if (window.confirm(`Remove ${x.display_name} from this organization?`))
                                  void act(() => api.delete(`/orgs/${org}/members/${x.user_id}`));
                              }}
                            >
                              Remove
                            </button>
                          )}
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TableWrap>
        )}
      </section>
      {manage && (
        <section className="card stack" aria-labelledby="inv-h">
          <h2 id="inv-h">Invitations</h2>
          {invitations.data?.length === 0 ? (
            <p className="muted">No invitations yet.</p>
          ) : (
            <TableWrap label="Invitations">
              <table>
                <thead>
                  <tr>
                    <th scope="col">Email</th>
                    <th scope="col">Role</th>
                    <th scope="col">Status</th>
                    <th scope="col">Expires</th>
                    <th scope="col">
                      <span className="visually-hidden">Actions</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {invitations.data?.map((i) => (
                    <tr key={i.id}>
                      <td>{i.email}</td>
                      <td>{ROLE_LABELS[i.role]}</td>
                      <td>
                        <StatusBadge status={i.status} />
                      </td>
                      <td className="small muted">{formatDate(i.expires_at)}</td>
                      <td>
                        {i.status === "pending" && (
                          <button
                            type="button"
                            className="btn small"
                            onClick={() => void act(() => api.delete(`/orgs/${org}/invitations/${i.id}`))}
                          >
                            Revoke
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableWrap>
          )}
        </section>
      )}
    </>
  );
}

function Invite({ org, roles, onDone }: { org: string; roles: Role[]; onDone: () => void }) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("contributor");
  const [link, setLink] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setLink(null);
    try {
      const r = await api.post<{ token: string }>(`/orgs/${org}/invitations`, { email, role });
      setLink(`${window.location.origin}/invitations/accept#${r.token}`);
      setEmail("");
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section className="card stack" aria-labelledby="invite-h">
      <h2 id="invite-h">Invite a member</h2>
      <form className="grid grid-3" onSubmit={submit}>
        <Field label="Email">
          {(p) => <input {...p} type="email" required value={email} onChange={(e) => setEmail(e.target.value)} />}
        </Field>
        <Field label="Role">
          {(p) => (
            <select {...p} value={role} onChange={(e) => setRole(e.target.value as Role)}>
              {roles.map((r) => (
                <option key={r} value={r}>
                  {ROLE_LABELS[r]}
                </option>
              ))}
            </select>
          )}
        </Field>
        <div className="field">
          <span className="label" aria-hidden="true">
            &nbsp;
          </span>
          <button type="submit" className="btn primary" disabled={!email.trim()}>
            Create invitation
          </button>
        </div>
      </form>
      {error && <ErrorNotice message={error} />}
      {link && (
        <div className="notice ok stack" role="status">
          <span>
            Invitation created. QUORUM does not send email yet — share this single-use link with the invitee through a
            trusted channel. It is shown only once.
          </span>
          <code className="prose">{link}</code>
        </div>
      )}
    </section>
  );
}

export function AcceptInvitationPage() {
  const { adopt } = useSession();
  const navigate = useNavigate();
  // The token travels in the URL fragment so it is not sent in Referer headers or server logs.
  const [token, setToken] = useState(() => window.location.hash.replace(/^#/, ""));
  const [error, setError] = useState<string | null>(null);

  async function accept(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const r = await api.post<{ organization_id: string }>("/invitations/accept", { token: token.trim() });
      adopt(await api.get<SessionView>("/auth/session"));
      navigate(`/o/${r.organization_id}`);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="auth">
      <main className="card" id="main">
        <h1>Accept invitation</h1>
        <p className="muted">You will join the organization with the role chosen by its administrator.</p>
        <form className="stack" onSubmit={accept}>
          {error && <ErrorNotice message={error} />}
          <Field label="Invitation token">
            {(p) => <input {...p} type="text" autoComplete="off" value={token} onChange={(e) => setToken(e.target.value)} />}
          </Field>
          <button className="btn primary" type="submit" disabled={!token.trim()}>
            Join organization
          </button>
        </form>
      </main>
    </div>
  );
}

export function NewOrganizationPage() {
  const { refresh } = useSession();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const o = await api.post<{ id: string }>("/orgs", { name });
      await refresh();
      navigate(`/o/${o.id}`);
    } catch (err) {
      setError(errorMessage(err));
    }
  }
  return (
    <div className="auth">
      <main className="card" id="main">
        <h1>New organization</h1>
        <p className="muted">Each organization is a separate data boundary. You will be its owner.</p>
        <form className="stack" onSubmit={submit}>
          {error && <ErrorNotice message={error} />}
          <Field label="Organization name">
            {(p) => <input {...p} type="text" maxLength={120} value={name} onChange={(e) => setName(e.target.value)} />}
          </Field>
          <div className="row">
            <button className="btn primary" type="submit" disabled={!name.trim()}>
              Create organization
            </button>
            <button className="btn ghost" type="button" onClick={() => navigate(-1)}>
              Cancel
            </button>
          </div>
        </form>
      </main>
    </div>
  );
}

export function ProvidersPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const [params, setParams] = useSearchParams();
  const offset = Number(params.get("offset") ?? 0) || 0;
  const catalog = useLoad((s) => api.get<Catalog>(`/orgs/${org}/providers`, s), [org]);
  const usage = useLoad(
    (s) => api.get<Paged<UsageRecord>>(`/orgs/${org}/provider-runs?limit=50&offset=${offset}`, s),
    [org, offset],
  );
  const rows = usage.data?.items ?? [];
  const live = rows.filter((r) => r.mode === "live");
  const tokens = live.reduce((acc, r) => acc + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0);

  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Organization</span>
          <h1>Providers &amp; usage</h1>
          <p>Provider keys and models are configured on the server by the operator; they are never sent to the browser.</p>
        </div>
      </div>
      <section className="card stack" aria-labelledby="prov-h">
        <h2 id="prov-h">Provider configuration</h2>
        {catalog.loading ? (
          <Loading />
        ) : (
          <TableWrap label="Provider configuration">
            <table>
              <thead>
                <tr>
                  <th scope="col">Provider</th>
                  <th scope="col">Status</th>
                  <th scope="col">Model</th>
                  <th scope="col">Data handling</th>
                </tr>
              </thead>
              <tbody>
                {catalog.data?.providers.map((p) => (
                  <tr key={p.id}>
                    <td>
                      <strong>{p.vendor}</strong>
                      <br />
                      <span className="small muted">{p.product}</span>
                    </td>
                    <td>
                      <StatusBadge status={p.status} label={p.status === "configured" ? "Configured · unverified" : undefined} />
                    </td>
                    <td>
                      <code>{p.model ?? "—"}</code>
                    </td>
                    <td className="small">{p.data_handling}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableWrap>
        )}
      </section>
      <section className="card stack" aria-labelledby="usage-h">
        <div className="row-between">
          <h2 id="usage-h">Usage records</h2>
          <span className="small muted">
            This page: {live.length} live requests · {tokens.toLocaleString()} tokens reported by providers
          </span>
        </div>
        <p className="small muted">
          Measured values from the usage ledger. Fixture requests involve no provider and no tokens. Cost estimates are not shown
          until pricing is configured.
        </p>
        {usage.loading ? (
          <Loading />
        ) : usage.error ? (
          <ErrorNotice message={usage.error} />
        ) : rows.length === 0 ? (
          <Empty title="No provider requests yet" />
        ) : (
          <TableWrap label="Usage records">
            <table>
              <thead>
                <tr>
                  <th scope="col">When</th>
                  <th scope="col">Who</th>
                  <th scope="col">Provider</th>
                  <th scope="col">Mode</th>
                  <th scope="col">Outcome</th>
                  <th scope="col">Tokens in/out</th>
                  <th scope="col">Latency</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.id}>
                    <td className="small">{formatDate(r.created_at)}</td>
                    <td>{r.user_name}</td>
                    <td>
                      {r.provider} <span className="small muted">{r.model}</span>
                    </td>
                    <td>{humanize(r.mode)}</td>
                    <td>
                      <StatusBadge status={r.outcome} />
                    </td>
                    <td>
                      {r.input_tokens ?? "—"} / {r.output_tokens ?? "—"}
                    </td>
                    <td>{r.mode === "live" ? `${r.latency_ms} ms` : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableWrap>
        )}
        <Pager offset={offset} hasMore={usage.data?.has_more ?? false} onChange={(o) => setParams({ offset: String(o) })} />
      </section>
    </>
  );
}

export function AuditPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const [params, setParams] = useSearchParams();
  const offset = Number(params.get("offset") ?? 0) || 0;
  const events = useLoad(
    (s) => api.get<Paged<AuditEvent>>(`/orgs/${org}/audit-events?limit=50&offset=${offset}`, s),
    [org, offset],
  );
  return (
    <>
      <div className="page-head">
        <div>
          <span className="eyebrow">Organization</span>
          <h1>Audit records</h1>
          <p>Append-only for the application: it can add records but cannot change or delete them.</p>
        </div>
      </div>
      <section className="card stack">
        {events.loading ? (
          <Loading />
        ) : events.error ? (
          <ErrorNotice message={events.error} />
        ) : (
          <TableWrap label="Audit records">
            <table>
              <thead>
                <tr>
                  <th scope="col">When</th>
                  <th scope="col">Actor</th>
                  <th scope="col">Event</th>
                  <th scope="col">Outcome</th>
                  <th scope="col">Details</th>
                  <th scope="col">Correlation</th>
                </tr>
              </thead>
              <tbody>
                {events.data?.items.map((e) => (
                  <tr key={e.id}>
                    <td className="small">{formatDate(e.occurred_at)}</td>
                    <td>{e.actor_name ?? "—"}</td>
                    <td>
                      <code>{e.event_type}</code>
                    </td>
                    <td>
                      <StatusBadge status={e.outcome} />
                    </td>
                    <td className="small">
                      <code>{Object.keys(e.details).length ? JSON.stringify(e.details) : ""}</code>
                    </td>
                    <td className="small muted">
                      <code>{e.correlation_id.slice(0, 8)}</code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableWrap>
        )}
        <Pager offset={offset} hasMore={events.data?.has_more ?? false} onChange={(o) => setParams({ offset: String(o) })} />
      </section>
    </>
  );
}

function Pager({ offset, hasMore, onChange }: { offset: number; hasMore: boolean; onChange: (o: number) => void }) {
  if (offset === 0 && !hasMore) return null;
  return (
    <nav className="row" aria-label="Pagination">
      <button type="button" className="btn small" disabled={offset === 0} onClick={() => onChange(Math.max(0, offset - 50))}>
        Newer
      </button>
      <button type="button" className="btn small" disabled={!hasMore} onClick={() => onChange(offset + 50)}>
        Older
      </button>
    </nav>
  );
}
