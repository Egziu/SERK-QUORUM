import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Outlet, Route, Routes } from "react-router";
import type { Membership } from "../api/types";
import { ConnectPage, parseImportFile } from "./Connect";

const ORG = "00000000-0000-0000-0000-0000000000aa";
const PROJECT = "00000000-0000-0000-0000-0000000000bb";
const BRIEF = "00000000-0000-0000-0000-0000000000cc";

const membership: Membership = {
  organization_id: ORG,
  organization_name: "Synthetic Org",
  role: "owner",
  permissions: ["workspace_read", "providers_use", "decisions_write", "decisions_decide", "documents_write"],
};

const catalog = {
  providers: [
    { id: "openai", vendor: "OpenAI", web_product: "ChatGPT", website_url: "https://chatgpt.com/", web_status: "available", api_status: "disabled", api_model: null, data_handling: "" },
    { id: "xai", vendor: "xAI", web_product: "Grok", website_url: "https://grok.com/", web_status: "available", api_status: "disabled", api_model: null, data_handling: "" },
  ],
  roles: [
    { key: "architect", name: "Architect", focus: "", guidance: "", built_in: true, suggested_provider: "openai" },
    { key: "challenger", name: "Challenger", focus: "", guidance: "", built_in: true, suggested_provider: "xai" },
  ],
  classifications: ["public", "internal", "confidential", "restricted"],
  external_share_limit: "internal",
  notice: "QUORUM cannot see or control what you do on external websites.",
};

const handoff = {
  id: "h1",
  brief_id: BRIEF,
  brief_version: 1,
  provider: "openai",
  role_name: "Architect",
  mode: "web",
  instruction: "# Role: Architect\nSynthetic instruction",
  instruction_sha256: "a".repeat(64),
  context_items: [],
  status: "prepared",
  website_url: "https://chatgpt.com/",
  opened_at: null,
  created_at: "2026-11-15T00:00:00Z",
};

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

function routes(url: string, init?: RequestInit): Response {
  const page = (items: unknown[]) => json({ items, limit: 100, offset: 0, has_more: false });
  if (url.endsWith(`/orgs/${ORG}/connect`)) return json(catalog);
  if (url.includes(`/orgs/${ORG}/projects?`))
    return page([{ id: PROJECT, name: "Pump station", description: "", status: "active", created_by: "u", created_at: "", updated_at: "", document_count: 0, decision_count: 0 }]);
  if (url.includes("/briefs?"))
    return page([{ id: BRIEF, project_id: PROJECT, title: "Control upgrade", mode: "quick", classification: "internal", version: 1, content: { request: "x" }, created_at: "", updated_at: "" }]);
  if (url.endsWith(`/briefs/${BRIEF}/handoffs`)) return json([handoff]);
  if (url.endsWith("/handoffs/h1/opened")) return json({ ...handoff, status: "opened" });
  if (url.includes("/responses") && init?.method === "POST")
    return json({ error: { code: "duplicate_response", message: "This exact response is already in the project.", existing_response_id: "r-existing-1" } }, 409);
  if (url.includes("/responses?") || url.includes("/documents?") || url.includes("/decisions?")) return page([]);
  return json({ error: { code: "not_found", message: "Endpoint not found." } }, 404);
}

function renderAt(path: string) {
  const fetch = vi.fn((url: string, init?: RequestInit) => Promise.resolve(routes(url, init)));
  vi.stubGlobal("fetch", fetch);
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<Outlet context={membership} />}>
          <Route path="/o/:org/connect" element={<ConnectPage />} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
  return fetch;
}

afterEach(() => vi.unstubAllGlobals());

describe("parseImportFile", () => {
  it("reads plain text and markdown unchanged", () => {
    expect(parseImportFile("answer.md", "# Heading\nBody")).toEqual({ text: "# Heading\nBody" });
  });

  it("reads the QUORUM JSON format", () => {
    const raw = JSON.stringify({ format: "quorum.response.v1", text: "Answer", provider: "openai", model: "m" });
    expect(parseImportFile("a.json", raw)).toEqual({ text: "Answer", provider: "openai", model: "m" });
  });

  it("rejects other JSON instead of guessing", () => {
    expect(() => parseImportFile("a.json", '{"messages":[]}')).toThrow(/QUORUM format/);
    expect(() => parseImportFile("a.json", "{not json")).toThrow(/could not be read/);
  });
});

describe("Web Connect handoff", () => {
  it("opens the official site in a new tab without leaking context or the brief", async () => {
    const fetch = renderAt(`/o/${ORG}/connect?project=${PROJECT}&brief=${BRIEF}`);
    const card = await screen.findByRole("article", { name: "ChatGPT · Architect" });
    const link = within(card).getByRole("link", { name: /Open ChatGPT/ });
    expect(link).toHaveAttribute("href", "https://chatgpt.com/");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link.getAttribute("rel")).toContain("noopener");
    expect(link.getAttribute("rel")).toContain("noreferrer");
    expect(link.getAttribute("href")).not.toContain("?");
    // Honest state: the card never claims a connection or a signed-in account.
    expect(card).not.toHaveTextContent(/connected|signed in|verified/i);
    expect(within(card).getByText("Ready to copy")).toBeInTheDocument();

    // Opening records the fact; it sends no prompt anywhere.
    link.addEventListener("click", (e) => e.preventDefault());
    await userEvent.setup().click(link);
    const opened = fetch.mock.calls.find(([u]) => String(u).endsWith("/handoffs/h1/opened"));
    expect(opened?.[1]?.method).toBe("POST");
    expect(fetch.mock.calls.every(([u]) => String(u).startsWith("/api/v1/"))).toBe(true);
  });

  it("explains a duplicate import instead of adding it twice", async () => {
    renderAt(`/o/${ORG}/connect?project=${PROJECT}&brief=${BRIEF}`);
    const card = await screen.findByRole("article", { name: "ChatGPT · Architect" });
    const user = userEvent.setup();
    await user.type(within(card).getByLabelText("Paste the ChatGPT answer"), "Synthetic answer");
    await user.click(within(card).getByRole("button", { name: "Import answer" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("already in the project");
    expect(within(card).getByText(/Nothing was added/)).toBeInTheDocument();
  });

  it("warns when the chosen classification exceeds the sharing policy", async () => {
    renderAt(`/o/${ORG}/connect?project=${PROJECT}`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "New brief" }));
    await user.selectOptions(screen.getByLabelText("Classification"), "confidential");
    expect(screen.getByText(/cannot be shared with external AI providers/)).toBeInTheDocument();
  });
});
