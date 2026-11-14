import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { ApiError, api, setCsrfToken, setUnauthenticatedHandler } from "./api/client";
import type { Membership, Permission, SessionView } from "./api/types";

interface SessionState {
  status: "loading" | "anonymous" | "authenticated";
  session: SessionView | null;
  refresh: () => Promise<void>;
  adopt: (s: SessionView) => void;
  signOut: () => Promise<void>;
}

const SessionContext = createContext<SessionState | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<SessionView | null>(null);
  const [status, setStatus] = useState<SessionState["status"]>("loading");

  const adopt = useCallback((s: SessionView) => {
    setCsrfToken(s.csrf_token);
    setSession(s);
    setStatus("authenticated");
  }, []);

  const clear = useCallback(() => {
    setCsrfToken(null);
    setSession(null);
    setStatus("anonymous");
  }, []);

  const refresh = useCallback(async () => {
    try {
      adopt(await api.get<SessionView>("/auth/session"));
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) clear();
      else throw e;
    }
  }, [adopt, clear]);

  const signOut = useCallback(async () => {
    try {
      await api.post("/auth/logout");
    } finally {
      clear();
    }
  }, [clear]);

  useEffect(() => {
    setUnauthenticatedHandler(clear);
    refresh().catch(() => setStatus("anonymous"));
    return () => setUnauthenticatedHandler(null);
  }, [refresh, clear]);

  const value = useMemo(() => ({ status, session, refresh, adopt, signOut }), [status, session, refresh, adopt, signOut]);
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionState {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside SessionProvider");
  return ctx;
}

/** The caller's membership in an organization, if any. */
export function useMembership(orgId: string | undefined): Membership | undefined {
  const { session } = useSession();
  return session?.memberships.find((m) => m.organization_id === orgId);
}

/** Hides controls the role cannot use. Display only; the server enforces. */
export function can(m: Membership | undefined, p: Permission): boolean {
  return m?.permissions.includes(p) ?? false;
}
