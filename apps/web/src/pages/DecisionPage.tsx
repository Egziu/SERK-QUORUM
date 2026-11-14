import { useState, type FormEvent } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { ApiError, api, errorMessage } from "../api/client";
import type { Decision, DecisionContent, DecisionStatus, DecisionVersion, Member, Membership, Paged } from "../api/types";
import { ErrorNotice, Field, Loading, StatusBadge } from "../components/ui";
import { formatDate, useLoad } from "../hooks";
import { can } from "../session";

const SECTIONS: [keyof DecisionContent, string, string][] = [
  ["problem_statement", "Problem statement", "What needs deciding, and why now?"],
  ["context", "Context", "Background, constraints and stakeholders."],
  ["alternatives", "Alternatives considered", "Required before review."],
  ["evaluation_criteria", "Evaluation criteria", "How the alternatives were compared."],
  ["evidence", "Supporting evidence", "Sources and measurements. Do not cite AI output as evidence."],
  ["outcome", "Decision", "What was decided. Required before review."],
  ["consequences", "Expected consequences", ""],
  ["risks", "Risks", ""],
];

export function DecisionPage() {
  const m = useOutletContext<Membership>();
  const org = m.organization_id;
  const { decision: id } = useParams();
  const decision = useLoad((s) => api.get<Decision>(`/orgs/${org}/decisions/${id}`, s), [org, id]);
  const history = useLoad((s) => api.get<DecisionVersion[]>(`/orgs/${org}/decisions/${id}/versions`, s), [org, id]);

  if (decision.loading && !decision.data) return <Loading />;
  if (decision.error || !decision.data) return <ErrorNotice message={decision.error ?? "Decision not found."} />;
  const d = decision.data;
  const reload = () => {
    decision.reload();
    history.reload();
  };

  return (
    <>
      <div className="page-head">
        <div>
          <Link to={`/o/${org}/projects/${d.project_id}?tab=decisions`} className="eyebrow">
            ← Decision register
          </Link>
          <h1>
            <code>{d.reference}</code> {d.title}
          </h1>
          <p>
            Version {d.version} · updated {formatDate(d.updated_at)}
          </p>
        </div>
        <StatusBadge status={d.status} />
      </div>

      <div className="split">
        <div className="stack">
          {d.status === "draft" && can(m, "decisions_write") ? (
            <Edit key={d.version} org={org} d={d} onSaved={reload} />
          ) : (
            <section className="card stack">
              {d.status !== "draft" && (
                <p className="small muted">
                  {d.status === "under_review"
                    ? "Content is frozen while under review. Return it to draft to edit."
                    : "This record is final. Its content can no longer be changed."}
                </p>
              )}
              {SECTIONS.map(([k, label]) => (
                <div className="section" key={k}>
                  <h3>{label}</h3>
                  <div className="prose">{d[k] || <span className="muted">Not recorded</span>}</div>
                </div>
              ))}
            </section>
          )}
        </div>
        <aside className="stack" aria-label="Decision details">
          <section className="card">
            <dl className="kv">
              <dt>Status</dt>
              <dd>
                <StatusBadge status={d.status} />
              </dd>
              <dt>Decision maker</dt>
              <dd>{d.decision_maker_name ?? "Not assigned"}</dd>
              <dt>Review date</dt>
              <dd>{d.review_date ?? "None"}</dd>
              {d.superseded_by && (
                <>
                  <dt>Superseded by</dt>
                  <dd>
                    <Link to={`/o/${org}/decisions/${d.superseded_by}`}>View replacement</Link>
                  </dd>
                </>
              )}
            </dl>
          </section>
          <Transitions m={m} d={d} onDone={reload} />
          <section className="card stack" aria-labelledby="dh">
            <h2 id="dh">History</h2>
            {history.loading ? (
              <Loading />
            ) : (
              <ol className="timeline">
                {history.data?.map((v) => (
                  <li key={v.version}>
                    <div className="row-between">
                      <strong>v{v.version}</strong>
                      <StatusBadge status={v.status} />
                    </div>
                    <div className="small">{v.note}</div>
                    <div className="small muted">
                      {v.changed_by_name} · {formatDate(v.changed_at)}
                    </div>
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

function Edit({ org, d, onSaved }: { org: string; d: Decision; onSaved: () => void }) {
  const [content, setContent] = useState<DecisionContent>({
    title: d.title,
    problem_statement: d.problem_statement,
    context: d.context,
    alternatives: d.alternatives,
    evaluation_criteria: d.evaluation_criteria,
    evidence: d.evidence,
    outcome: d.outcome,
    consequences: d.consequences,
    risks: d.risks,
  });
  const [maker, setMaker] = useState(d.decision_maker_id ?? "");
  const [review, setReview] = useState(d.review_date ?? "");
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const members = useLoad((s) => api.get<Member[]>(`/orgs/${org}/members`, s), [org]);
  const deciders = (members.data ?? []).filter((x) =>
    ["owner", "administrator", "project_manager", "reviewer"].includes(x.role),
  );

  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.patch(`/orgs/${org}/decisions/${d.id}`, {
        base_version: d.version,
        ...content,
        decision_maker_id: maker || null,
        review_date: review || null,
      });
      onSaved();
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError(0, "unknown", errorMessage(err)));
      setBusy(false);
    }
  }

  const err = (f: string) => (error?.field === f ? error.message : null);
  return (
    <form className="card stack" onSubmit={save} aria-label="Edit decision">
      {error && !error.field && <ErrorNotice message={error.message} />}
      <Field label="Title" error={err("title")}>
        {(p) => (
          <input {...p} type="text" maxLength={200} value={content.title} onChange={(e) => setContent({ ...content, title: e.target.value })} />
        )}
      </Field>
      {SECTIONS.map(([k, label, hint]) => (
        <Field key={k} label={label} {...(hint ? { hint } : {})} error={err(k)}>
          {(p) => <textarea {...p} value={content[k]} onChange={(e) => setContent({ ...content, [k]: e.target.value })} />}
        </Field>
      ))}
      <div className="grid grid-2">
        <Field label="Responsible decision maker" hint="Members who may approve decisions." error={err("decision_maker_id")}>
          {(p) => (
            <select {...p} value={maker} onChange={(e) => setMaker(e.target.value)}>
              <option value="">Not assigned</option>
              {deciders.map((x) => (
                <option key={x.user_id} value={x.user_id}>
                  {x.display_name}
                </option>
              ))}
            </select>
          )}
        </Field>
        <Field label="Review date" error={err("review_date")}>
          {(p) => <input {...p} type="date" value={review} onChange={(e) => setReview(e.target.value)} />}
        </Field>
      </div>
      <div>
        <button type="submit" className="btn primary" disabled={busy}>
          {busy ? "Saving…" : "Save draft"}
        </button>
      </div>
    </form>
  );
}

function Transitions({ m, d, onDone }: { m: Membership; d: Decision; onDone: () => void }) {
  const org = m.organization_id;
  const [note, setNote] = useState("");
  const [confirm, setConfirm] = useState(false);
  const [supersededBy, setSupersededBy] = useState("");
  const [error, setError] = useState<string | null>(null);
  const approved = useLoad(
    (s) =>
      d.status === "approved"
        ? api.get<Paged<Decision>>(`/orgs/${org}/projects/${d.project_id}/decisions?limit=100`, s)
        : Promise.resolve(null),
    [org, d.project_id, d.status],
  );
  const write = can(m, "decisions_write");
  const decide = can(m, "decisions_decide");

  const options: { to: DecisionStatus; label: string; decider: boolean }[] = [];
  if (d.status === "draft" && write) options.push({ to: "under_review", label: "Submit for review", decider: false });
  if (d.status === "under_review" && write) options.push({ to: "draft", label: "Return to draft", decider: false });
  if (d.status === "under_review" && decide) {
    options.push({ to: "approved", label: "Approve", decider: true });
    options.push({ to: "rejected", label: "Reject", decider: true });
  }
  if (d.status === "rejected" && write) options.push({ to: "draft", label: "Reopen as draft", decider: false });
  if (d.status === "approved" && decide) options.push({ to: "superseded", label: "Mark superseded", decider: true });
  if (options.length === 0) return null;

  const replacements = (approved.data?.items ?? []).filter((x) => x.id !== d.id && x.status === "approved");

  async function go(to: DecisionStatus) {
    setError(null);
    try {
      await api.post(`/orgs/${org}/decisions/${d.id}/transition`, {
        to,
        base_version: d.version,
        note,
        confirm,
        ...(to === "superseded" ? { superseded_by: supersededBy || null } : {}),
      });
      setNote("");
      setConfirm(false);
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const needsConfirm = options.some((o) => o.decider);
  return (
    <section className="card stack" aria-labelledby="actions-h">
      <h2 id="actions-h">Actions</h2>
      {error && <ErrorNotice message={error} />}
      <Field label="Note" hint="Recorded in the history.">
        {(p) => <input {...p} type="text" maxLength={500} value={note} onChange={(e) => setNote(e.target.value)} />}
      </Field>
      {d.status === "approved" && decide && (
        <Field label="Superseded by" hint="An approved decision in this project.">
          {(p) => (
            <select {...p} value={supersededBy} onChange={(e) => setSupersededBy(e.target.value)}>
              <option value="">Choose…</option>
              {replacements.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.reference} {x.title}
                </option>
              ))}
            </select>
          )}
        </Field>
      )}
      {needsConfirm && (
        <label className="check">
          <input type="checkbox" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} />
          I confirm this is a human decision I am accountable for. AI agreement is not verification.
        </label>
      )}
      <div className="row">
        {options.map((o) => (
          <button
            key={o.to}
            type="button"
            className={o.to === "approved" ? "btn primary" : o.to === "rejected" ? "btn danger" : "btn"}
            disabled={(o.decider && !confirm) || (o.to === "superseded" && !supersededBy)}
            onClick={() => void go(o.to)}
          >
            {o.label}
          </button>
        ))}
      </div>
    </section>
  );
}
