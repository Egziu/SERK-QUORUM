import { ApiError, api, errorMessage, setCsrfToken } from "./client";

function mockFetch(status: number, body: unknown) {
  const fn = vi.fn().mockImplementation(() =>
    Promise.resolve(
      new Response(body === undefined ? null : JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
  vi.stubGlobal("fetch", fn);
  return fn;
}

afterEach(() => {
  vi.unstubAllGlobals();
  setCsrfToken(null);
});

it("sends the CSRF token on state-changing requests only", async () => {
  setCsrfToken("csrf-123");
  const fetch = mockFetch(200, { ok: true });
  await api.post("/orgs", { name: "x" });
  await api.get("/orgs/1");
  const post = fetch.mock.calls[0]?.[1] as RequestInit;
  const get = fetch.mock.calls[1]?.[1] as RequestInit;
  expect((post.headers as Record<string, string>)["x-csrf-token"]).toBe("csrf-123");
  expect((get.headers as Record<string, string>)["x-csrf-token"]).toBeUndefined();
  expect(post.credentials).toBe("same-origin");
  expect(fetch.mock.calls[0]?.[0]).toBe("/api/v1/orgs");
});

it("parses the error envelope", async () => {
  mockFetch(422, {
    error: { code: "validation_failed", message: "name: too long", field: "name", correlation_id: "abc" },
  });
  const err = await api.post("/x", {}).catch((e: unknown) => e);
  expect(err).toBeInstanceOf(ApiError);
  const e = err as ApiError;
  expect(e.status).toBe(422);
  expect(e.field).toBe("name");
  expect(errorMessage(e)).toBe("name: too long");
});

it("adds the correlation id to server errors", async () => {
  mockFetch(500, { error: { code: "internal", message: "Something went wrong.", correlation_id: "ref-9" } });
  const e = (await api.get("/x").catch((x: unknown) => x)) as ApiError;
  expect(errorMessage(e)).toContain("ref-9");
});

it("reports network failures distinctly", async () => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
  const e = (await api.get("/x").catch((x: unknown) => x)) as ApiError;
  expect(e.isNetwork).toBe(true);
  expect(e.message).toMatch(/Cannot reach/);
});

it("handles 204 No Content", async () => {
  mockFetch(204, undefined);
  await expect(api.delete("/x")).resolves.toBeUndefined();
});
