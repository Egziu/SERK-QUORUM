import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Outlet, Route, Routes } from "react-router";
import type { Membership } from "../api/types";
import { SessionProvider } from "../session";
import { ProvidersPage } from "./Admin";

const ORG = "00000000-0000-0000-0000-0000000000aa";

function membership(permissions: Membership["permissions"]): Membership {
  return { organization_id: ORG, organization_name: "Synthetic Org", role: "owner", permissions };
}

const base = {
  vendor: "OpenAI",
  product: "Responses API",
  perspective: "Architect",
  status: "configured",
  model: "synthetic-model",
  capabilities: ["text"],
  data_handling: "",
  fixture_available: true,
};

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function renderPage(m: Membership, checks: unknown[]) {
  let lastCheck: unknown = null;
  const fetch = vi.fn((url: string, init?: RequestInit) => {
    if (url.endsWith("/auth/session")) return Promise.resolve(json({ error: { code: "unauthenticated", message: "x" } }, 401));
    if (url.endsWith("/provider-checks") && init?.method === "POST") {
      lastCheck = checks.shift();
      return Promise.resolve(json(lastCheck));
    }
    if (url.endsWith("/providers"))
      return Promise.resolve(
        json({ providers: [{ id: "openai", ...base, last_check: lastCheck }], tasks: [], prompt_max_chars: 1, live_enabled: true }),
      );
    if (url.includes("/provider-runs")) return Promise.resolve(json({ items: [], limit: 50, offset: 0, has_more: false }));
    return Promise.resolve(json({ error: { code: "not_found", message: "x" } }, 404));
  });
  vi.stubGlobal("fetch", fetch);
  render(
    <MemoryRouter initialEntries={[`/o/${ORG}/providers`]}>
      <SessionProvider>
        <Routes>
          <Route element={<Outlet context={m} />}>
            <Route path="/o/:org/providers" element={<ProvidersPage />} />
          </Route>
        </Routes>
      </SessionProvider>
    </MemoryRouter>,
  );
  return fetch;
}

afterEach(() => vi.unstubAllGlobals());

it("tests a provider connection and shows an honest result with advice", async () => {
  const fetch = renderPage(membership(["workspace_read", "members_manage", "usage_read"]), [
    {
      provider: "openai",
      model: "synthetic-model",
      outcome: "key_rejected",
      http_status: 401,
      latency_ms: 120,
      reply: null,
      advice: "The provider rejected the API key (401/403).",
      checked_at: "2026-11-15T06:00:00Z",
      checked_by: "Ada",
    },
  ]);
  const table = await screen.findByRole("region", { name: "Provider configuration" });
  expect(within(table).getByText("Not checked since the server started")).toBeInTheDocument();
  await userEvent.setup().click(within(table).getByRole("button", { name: "Test OpenAI" }));
  expect(await within(table).findByText("Key rejected (HTTP 401)")).toBeInTheDocument();
  expect(within(table).getByText(/rejected the API key/)).toBeInTheDocument();
  const call = fetch.mock.calls.find(([u]) => String(u).endsWith("/provider-checks"));
  expect(JSON.parse(String(call?.[1]?.body))).toEqual({ provider: "openai" });
  // The setup guide names the server variables, never a key value.
  expect(screen.getByText("QUORUM_PROVIDERS_LIVE=1")).toBeInTheDocument();
});

it("hides the test button from people who cannot manage the organization", async () => {
  renderPage(membership(["workspace_read", "usage_read"]), []);
  const table = await screen.findByRole("region", { name: "Provider configuration" });
  expect(within(table).queryByRole("button", { name: /Test/ })).not.toBeInTheDocument();
});
