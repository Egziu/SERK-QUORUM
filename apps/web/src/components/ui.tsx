import { useId, type ReactNode } from "react";

export function Loading({ label = "Loading…" }: { label?: string }) {
  return (
    <div className="row muted" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

export function ErrorNotice({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="notice error row-between" role="alert">
      <span>{message}</span>
      {onRetry && (
        <button type="button" className="btn small" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children}
    </div>
  );
}

const STATUS_TONE: Record<string, "ok" | "warn" | "danger" | undefined> = {
  active: "ok",
  approved: "ok",
  configured: "ok",
  succeeded: "ok",
  success: "ok",
  accepted: "ok",
  pending: "warn",
  under_review: "warn",
  on_hold: "warn",
  not_configured: "warn",
  draft: undefined,
  rejected: "danger",
  failed: "danger",
  denied: "danger",
  failure: "danger",
  revoked: "danger",
  expired: undefined,
  superseded: undefined,
  archived: undefined,
  completed: "ok",
  disabled: undefined,
  awaiting_responses: "warn",
  ready_for_review: "warn",
  decision_recorded: "ok",
  imported: "ok",
  opened: "warn",
  prepared: undefined,
  unreviewed: undefined,
  public: undefined,
  internal: undefined,
  confidential: "warn",
  restricted: "danger",
};

export function humanize(s: string): string {
  const t = s.replace(/_/g, " ");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

export function StatusBadge({ status, label }: { status: string; label?: string | undefined }) {
  const tone = STATUS_TONE[status];
  return <span className={`badge${tone ? ` ${tone}` : ""}`}>{label ?? humanize(status)}</span>;
}

interface FieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  children: (props: { id: string; "aria-describedby"?: string; "aria-invalid"?: true }) => ReactNode;
}

/** Associates a label, hint and error message with a control. */
export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children({
        id,
        ...(describedBy ? { "aria-describedby": describedBy } : {}),
        ...(error ? { "aria-invalid": true as const } : {}),
      })}
      {hint && (
        <span className="hint" id={hintId}>
          {hint}
        </span>
      )}
      {error && (
        <span className="field-error" id={errorId}>
          {error}
        </span>
      )}
    </div>
  );
}

/** Horizontally scrollable table container. Focusable and labelled so
 *  keyboard users can scroll it (WCAG 2.1.1). */
export function TableWrap({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="table-wrap" role="region" aria-label={label} tabIndex={0}>
      {children}
    </div>
  );
}
