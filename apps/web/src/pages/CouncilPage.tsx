import { useEffect, useState, type FormEvent } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router";
import { api, errorMessage } from "../api/client";
import {
  ANNOTATION_KINDS,
  type AiResponse,
  type AnnotationKind,
  type CouncilDetail,
  type CouncilStatus,
  type Membership,
  type ReviewStatus,
} from "../api/types";
import { ErrorNotice, Field, Loading, StatusBadge, humanize } from "../components/ui";
import { formatDate, useLoad } from "../hooks";
import { can, useSession } from "../session";

const NEXT: Partial<Record<CouncilStatus, [CouncilStatus, string][]>> = {
  draft: [["archived", "Archive"]],
  awaiting_responses: [["ready_for_review", "Mark ready for review"]],
  ready_for_review: [
    ["under_review", "Start review"],
    ["awaiting_responses", "Wait for more responses"],
  ],
  under_review: [["ready_for_review", "Pause review"]],
  decision_recorded: [["archived", "Archive"]],
};

type Layout = "columns" | "tabs";

function useNarrow(): boolean {
  const query = "(max-width: 760px)";
  const [narrow, setNarrow] = useState(() => typeof window !== "undefined" && window.matchMedia?.(query).matches);
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const on = () => setNarrow(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return narrow;
}

/** Council: people compare responses side by side, annotate and rate them,
 *  and record the decision. Nothing here is decided automatically. */
export function CouncilPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const { council: id } = useParams();
  const navigate = useNavigate();
  const narrow = useNarrow();
  const [layout, setLayout] = useState<Layout | null>(null);
  const [active, setActive] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState("");
  const [confirm, setConfirm] = useState(false);
  const council = useLoad((s) => api.get<CouncilDetail>(`/orgs/${org}/councils/${id}`, s), [org, id]);

  if (council.loading && !council.data) return <Loading />;
  if (council.error || !council.data) return <ErrorNotice message={council.error ?? "Council not found."} onRetry={council.reload} />;
  const c = council.data;
  const effective: Layout = layout ?? (narrow ? "tabs" : "columns");
  const base = `/orgs/${org}/councils/${c.id}`;

  async function act<T>(fn: () => Promise<T>) {
    setError(null);
    try {
      await fn();
      council.reload();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function exportDocument() {
    setError(null);
    try {
      const r = await api.post<{ document_id: string }>(`${base}/export-document`);
      navigate(`../../documents/${r.document_id}`, { relative: "path" });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const transitions = (NEXT[c.status] ?? []).filter(([to]) =>
    to === "archived" ? can(m, "decisions_decide") : can(m, "decisions_write"),
  );
  const canRecord = c.can_decide && (c.status === "ready_for_review" || c.status === "under_review");

  return (
    <>
      <div className="page-head">
        <div>
          <Link to={`../../projects/${c.project_id}?tab=councils`} relative="path" className="eyebrow">
            ← Project councils
          </Link>
          <h1>{c.title}</h1>
          {c.question && <p className="prose">{c.question}</p>}
          <p className="small muted">
            {c.members.length} responses · version {c.version} · updated {formatDate(c.updated_at)}
          </p>
        </div>
        <StatusBadge status={c.status} />
      </div>
      {error && <ErrorNotice message={error} />}
      {!c.can_edit && (c.status === "decision_recorded" || c.status === "archived") && (
        <div className="notice" role="status">
          This council is completed and frozen. Its record stays exactly as it was when the decision was made.
        </div>
      )}

      <div className="row">
        {transitions.map(([to, label]) => (
          <button
            key={to}
            type="button"
            className="btn"
            onClick={() => void act(() => api.post(`${base}/transition`, { to, base_version: c.version }))}
          >
            {label}
          </button>
        ))}
        <a className="btn" href={`/api/v1${base}/export.md`} download>
          Export Markdown
        </a>
        {can(m, "documents_write") && (
          <button type="button" className="btn" onClick={() => void exportDocument()}>
            Save as document
          </button>
        )}
        <fieldset className="segmented" aria-label="Layout">
          {(["columns", "tabs"] as Layout[]).map((l) => (
            <label key={l}>
              <input type="radio" name="council-layout" checked={effective === l} onChange={() => setLayout(l)} />
              {l === "columns" ? "Side by side" : "One at a time"}
            </label>
          ))}
        </fieldset>
      </div>

      {c.members.length === 0 ? (
        <div className="empty">
          <strong>No responses yet</strong>
          <span>
            Import responses in <Link to={`../../connect?project=${c.project_id}`} relative="path">Connect</Link>, then add them here.
          </span>
        </div>
      ) : effective === "tabs" ? (
        <>
          <div className="tabs" role="tablist" aria-label="Responses">
            {c.members.map((x, i) => (
              <button
                key={x.response.id}
                type="button"
                role="tab"
                id={`resp-tab-${i}`}
                aria-selected={active === i}
                aria-controls="resp-panel"
                tabIndex={active === i ? 0 : -1}
                onClick={() => setActive(i)}
                onKeyDown={(e) => {
                  if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
                  const n = c.members.length;
                  const next = (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
                  setActive(next);
                  document.getElementById(`resp-tab-${next}`)?.focus();
                }}
              >
                {x.position}. {x.response.role_name ?? x.response.provider}
              </button>
            ))}
          </div>
          <div role="tabpanel" id="resp-panel" aria-labelledby={`resp-tab-${active}`}>
            {c.members[active] && <ResponseColumn m={m} council={c} member={c.members[active]} onChange={council.reload} />}
          </div>
        </>
      ) : (
        <div className="council-grid">
          {c.members.map((x) => (
            <ResponseColumn key={x.response.id} m={m} council={c} member={x} onChange={council.reload} />
          ))}
        </div>
      )}

      <section className="card stack" aria-labelledby="council-notes">
        <h2 id="council-notes">Council notes</h2>
        <Annotations council={c} responseId={null} m={m} onChange={council.reload} />
      </section>

      <section className="card stack" aria-labelledby="council-decision">
        <h2 id="council-decision">Decision</h2>
        {c.decision ? (
          <div className="stack">
            <p className="prose">{c.decision_summary}</p>
            <p>
              Recorded in the register as{" "}
              <Link to={`../../decisions/${c.decision.id}`} relative="path">
                {c.decision.reference}
              </Link>{" "}
              <StatusBadge status={c.decision.status} />. Approve it there through the normal decision workflow.
            </p>
          </div>
        ) : canRecord ? (
          <form
            className="stack"
            aria-label="Record decision"
            onSubmit={(e: FormEvent) => {
              e.preventDefault();
              void act(() => api.post(`${base}/decision`, { summary, base_version: c.version, confirm }));
            }}
          >
            <Field label="Decision" hint="State what was decided and why. This is your decision, not the AI's.">
              {(p) => <textarea {...p} rows={4} required value={summary} onChange={(e) => setSummary(e.target.value)} />}
            </Field>
            <label className="check">
              <input type="checkbox" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} />I reviewed the responses and
              take responsibility for this decision.
            </label>
            <div className="row">
              <button type="submit" className="btn primary" disabled={!summary.trim() || !confirm}>
                Record decision
              </button>
            </div>
          </form>
        ) : (
          <p className="muted">
            {c.can_decide
              ? "Compare at least two responses (ready for review) before recording a decision."
              : "Only members who may decide can record the decision."}
          </p>
        )}
      </section>
    </>
  );
}

function ResponseColumn({
  m,
  council,
  member,
  onChange,
}: {
  m: Membership;
  council: CouncilDetail;
  member: CouncilDetail["members"][number];
  onChange: () => void;
}) {
  const r: AiResponse = member.response;
  const { session } = useSession();
  const org = m.organization_id;
  const [error, setError] = useState<string | null>(null);
  const reviewer = can(m, "decisions_write") || can(m, "decisions_decide");
  const mine = council.ratings.filter((x) => x.response_id === r.id && x.rated_by === session?.user.id);
  const others = council.ratings.filter((x) => x.response_id === r.id && x.rated_by !== session?.user.id);

  async function review(status: ReviewStatus) {
    setError(null);
    try {
      await api.patch(`/orgs/${org}/responses/${r.id}`, { review_status: status });
      onChange();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function rate(criterion: string, score: number) {
    setError(null);
    try {
      await api.put(`/orgs/${org}/councils/${council.id}/ratings`, { response_id: r.id, criterion, score });
      onChange();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <article className="card stack response-column" aria-label={`Response ${member.position}: ${r.role_name ?? r.provider}`}>
      <header className="stack">
        <div className="row-between">
          <h2>
            {member.position}. {r.role_name ?? "Response"}
          </h2>
          <StatusBadge status={r.review_status} />
        </div>
        <span className="small muted">
          {r.provider}
          {r.model ? ` / ${r.model}` : ""} · {r.characters.toLocaleString()} characters
        </span>
        <span className="small" title={r.provenance_label}>
          <StatusBadge
            status={r.provenance === "provider_reported" ? "configured" : "pending"}
            label={r.provenance === "provider_reported" ? "Provider-reported" : "User-declared"}
          />{" "}
          {r.provenance_label}
        </span>
      </header>
      {error && <ErrorNotice message={error} />}
      <div className="response-body" tabIndex={0} role="region" aria-label={`Text of response ${member.position}`}>
        {r.body}
      </div>
      {reviewer && council.can_edit && (
        <Field label="Review">
          {(p) => (
            <select {...p} value={r.review_status} onChange={(e) => void review(e.target.value as ReviewStatus)}>
              <option value="unreviewed">Unreviewed</option>
              <option value="accepted">Accepted</option>
              <option value="rejected">Rejected</option>
            </select>
          )}
        </Field>
      )}
      <details>
        <summary>Ratings ({mine.length + others.length})</summary>
        <div className="stack">
          {council.criteria.map((cr) => {
            const own = mine.find((x) => x.criterion === cr.key);
            const theirs = others.filter((x) => x.criterion === cr.key);
            return (
              <div key={cr.key} className="row-between">
                {reviewer && council.can_edit ? (
                  <Field label={cr.label}>
                    {(p) => (
                      <select {...p} value={own?.score ?? ""} onChange={(e) => e.target.value && void rate(cr.key, Number(e.target.value))}>
                        <option value="">Not rated</option>
                        {[1, 2, 3, 4, 5].map((n) => (
                          <option key={n} value={n}>
                            {n} of 5
                          </option>
                        ))}
                      </select>
                    )}
                  </Field>
                ) : (
                  <span>
                    {cr.label}: {own ? `${own.score} of 5` : "not rated"}
                  </span>
                )}
                {theirs.length > 0 && (
                  <span className="small muted">
                    {theirs.map((x) => `${x.rated_by_name} ${x.score}`).join(", ")}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      </details>
      <Annotations council={council} responseId={r.id} m={m} onChange={onChange} />
    </article>
  );
}

function Annotations({
  council,
  responseId,
  m,
  onChange,
}: {
  council: CouncilDetail;
  responseId: string | null;
  m: Membership;
  onChange: () => void;
}) {
  const org = m.organization_id;
  const { session } = useSession();
  const [kind, setKind] = useState<AnnotationKind>("note");
  const [body, setBody] = useState("");
  const [quote, setQuote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const list = council.annotations.filter((a) => a.response_id === responseId);
  const reviewer = (can(m, "decisions_write") || can(m, "decisions_decide")) && council.can_edit;

  async function add(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await api.post(`/orgs/${org}/councils/${council.id}/annotations`, {
        response_id: responseId,
        kind,
        quote,
        body,
      });
      setBody("");
      setQuote("");
      onChange();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function remove(id: string) {
    setError(null);
    try {
      await api.delete(`/orgs/${org}/councils/${council.id}/annotations/${id}`);
      onChange();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="stack">
      {list.length > 0 && (
        <ul className="stack" aria-label="Annotations">
          {list.map((a) => (
            <li key={a.id} className="annotation">
              <StatusBadge status={a.kind === "risk" || a.kind === "contradiction" ? "pending" : "draft"} label={humanize(a.kind)} />{" "}
              {a.quote && <q className="small">{a.quote}</q>} <span>{a.body}</span>
              <br />
              <span className="small muted">
                {a.author_name} · {formatDate(a.created_at)}
              </span>
              {reviewer && a.author_id === session?.user.id && (
                <button type="button" className="btn ghost small" onClick={() => void remove(a.id)}>
                  Delete
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      {reviewer && (
        <form className="stack" onSubmit={add} aria-label={responseId ? "Annotate this response" : "Add a council note"}>
          {error && <ErrorNotice message={error} />}
          <div className="grid grid-2">
            <Field label="Kind">
              {(p) => (
                <select {...p} value={kind} onChange={(e) => setKind(e.target.value as AnnotationKind)}>
                  {ANNOTATION_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {humanize(k)}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            {responseId && (
              <Field label="Quoted passage (optional)">
                {(p) => <input {...p} type="text" maxLength={2000} value={quote} onChange={(e) => setQuote(e.target.value)} />}
              </Field>
            )}
          </div>
          <Field label={responseId ? "Annotation" : "Note"}>
            {(p) => <textarea {...p} rows={2} required value={body} onChange={(e) => setBody(e.target.value)} />}
          </Field>
          <div className="row">
            <button type="submit" className="btn small" disabled={!body.trim()}>
              Add
            </button>
          </div>
        </form>
      )}
    </div>
  );
}
