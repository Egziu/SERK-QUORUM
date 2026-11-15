// Thin fetch wrapper. The session lives in an HttpOnly cookie the script
// cannot read; the CSRF token is held in memory only (never localStorage).

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly field?: string,
    readonly correlationId?: string,
    /** The full error object, for endpoints that add details (e.g. duplicates). */
    readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "ApiError";
  }

  get isNetwork(): boolean {
    return this.status === 0;
  }
}

let csrfToken: string | null = null;
let onUnauthenticated: (() => void) | null = null;

export function setCsrfToken(token: string | null): void {
  csrfToken = token;
}

export function setUnauthenticatedHandler(handler: (() => void) | null): void {
  onUnauthenticated = handler;
}

interface ErrorEnvelope {
  error?: { code?: string; message?: string; field?: string; correlation_id?: string } & Record<string, unknown>;
}

async function request<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const headers: Record<string, string> = { accept: "application/json" };
  if (body !== undefined) headers["content-type"] = "application/json";
  if (method !== "GET" && csrfToken) headers["x-csrf-token"] = csrfToken;
  let res: Response;
  try {
    res = await fetch(`/api/v1${path}`, {
      method,
      headers,
      credentials: "same-origin",
      ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
      ...(signal ? { signal } : {}),
    });
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") throw e;
    throw new ApiError(0, "network", "Cannot reach the QUORUM server. Check your connection and try again.");
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  const data: unknown = text ? safeJson(text) : null;
  if (!res.ok) {
    const err = (data as ErrorEnvelope | null)?.error;
    if (res.status === 401 && onUnauthenticated && !path.startsWith("/auth/login")) onUnauthenticated();
    throw new ApiError(
      res.status,
      err?.code ?? "http_error",
      err?.message ?? `Request failed (${res.status}).`,
      err?.field,
      err?.correlation_id,
      err ?? undefined,
    );
  }
  return data as T;
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export const api = {
  get: <T>(path: string, signal?: AbortSignal) => request<T>("GET", path, undefined, signal),
  post: <T>(path: string, body: unknown = {}, signal?: AbortSignal) => request<T>("POST", path, body, signal),
  patch: <T>(path: string, body: unknown) => request<T>("PATCH", path, body),
  put: <T>(path: string, body: unknown) => request<T>("PUT", path, body),
  delete: <T>(path: string) => request<T>("DELETE", path),
};

export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    return e.correlationId && e.status >= 500 ? `${e.message} (ref ${e.correlationId})` : e.message;
  }
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}
