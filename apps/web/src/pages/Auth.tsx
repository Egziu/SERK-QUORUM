import { useState, type FormEvent } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { ApiError, api, errorMessage } from "../api/client";
import type { SessionView } from "../api/types";
import { ErrorNotice, Field } from "../components/ui";
import { useSession } from "../session";

function Brand({ subtitle }: { subtitle: string }) {
  return (
    <div className="auth-brand">
      <img src="/logo.svg" alt="SERK QUORUM" />
      <strong>QUORUM</strong>
      <span className="muted small">{subtitle}</span>
    </div>
  );
}

export function LoginPage() {
  const { adopt } = useSession();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const s = await api.post<SessionView>("/auth/login", { email, password });
      adopt(s);
      const next = params.get("next");
      navigate(next?.startsWith("/") && !next.startsWith("//") ? next : "/");
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 401 ? "Email or password is incorrect." : errorMessage(err),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="auth">
      <main className="card" id="main">
        <Brand subtitle="Intelligence. In concert." />
        <h1 className="visually-hidden">Sign in</h1>
        <form className="stack" onSubmit={submit} noValidate>
          {error && <ErrorNotice message={error} />}
          <Field label="Email">
            {(p) => (
              <input {...p} type="email" autoComplete="username" required value={email} onChange={(e) => setEmail(e.target.value)} />
            )}
          </Field>
          <Field label="Password">
            {(p) => (
              <input
                {...p}
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            )}
          </Field>
          <button className="btn primary" type="submit" disabled={busy || !email || !password}>
            {busy ? "Signing in…" : "Sign in"}
          </button>
        </form>
        <p className="small muted">
          New to QUORUM? <Link to="/signup">Create an organization</Link>
        </p>
      </main>
    </div>
  );
}

export function SignupPage() {
  const { adopt } = useSession();
  const navigate = useNavigate();
  const [form, setForm] = useState({ display_name: "", email: "", password: "", organization_name: "" });
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (k: keyof typeof form) => (e: { target: { value: string } }) => setForm({ ...form, [k]: e.target.value });
  const fieldError = (f: string) => (error?.field === f ? error.message : null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const s = await api.post<SessionView>("/auth/signup", form);
      adopt(s);
      navigate(`/o/${s.memberships[0]?.organization_id ?? ""}`);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError(0, "unknown", errorMessage(err)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="auth">
      <main className="card" id="main">
        <Brand subtitle="Create your organization workspace" />
        <h1 className="visually-hidden">Create account</h1>
        <form className="stack" onSubmit={submit} noValidate>
          {error && !error.field && (
            <ErrorNotice
              message={error.code === "forbidden" ? "Self-service sign-up is disabled on this server. Ask an administrator for an invitation." : error.message}
            />
          )}
          <Field label="Your name" error={fieldError("display_name")}>
            {(p) => <input {...p} type="text" autoComplete="name" value={form.display_name} onChange={set("display_name")} />}
          </Field>
          <Field label="Work email" error={fieldError("email")}>
            {(p) => <input {...p} type="email" autoComplete="email" value={form.email} onChange={set("email")} />}
          </Field>
          <Field label="Password" hint="At least 12 characters. A passphrase works well." error={fieldError("password")}>
            {(p) => <input {...p} type="password" autoComplete="new-password" value={form.password} onChange={set("password")} />}
          </Field>
          <Field label="Organization name" error={fieldError("organization_name")}>
            {(p) => <input {...p} type="text" autoComplete="organization" value={form.organization_name} onChange={set("organization_name")} />}
          </Field>
          <button className="btn primary" type="submit" disabled={busy}>
            {busy ? "Creating…" : "Create organization"}
          </button>
        </form>
        <p className="small muted">
          Already have an account? <Link to="/login">Sign in</Link>
        </p>
      </main>
    </div>
  );
}
