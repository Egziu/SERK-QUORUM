import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { SessionProvider } from "../session";
import { LoginPage } from "./Auth";

function json(status: number, body: unknown) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

afterEach(() => vi.unstubAllGlobals());

it("shows a generic message for wrong credentials", async () => {
  const fetch = vi
    .fn()
    // initial session probe
    .mockResolvedValueOnce(json(401, { error: { code: "unauthenticated", message: "Sign in to continue." } }))
    // login attempt
    .mockResolvedValueOnce(json(401, { error: { code: "unauthenticated", message: "Sign in to continue." } }));
  vi.stubGlobal("fetch", fetch);
  render(
    <MemoryRouter>
      <SessionProvider>
        <LoginPage />
      </SessionProvider>
    </MemoryRouter>,
  );
  const user = userEvent.setup();
  const submit = screen.getByRole("button", { name: "Sign in" });
  expect(submit).toBeDisabled();
  await user.type(screen.getByLabelText("Email"), "ada@example.test");
  await user.type(screen.getByLabelText("Password"), "wrong-password");
  await user.click(submit);
  expect(await screen.findByRole("alert")).toHaveTextContent("Email or password is incorrect.");
  const body = JSON.parse((fetch.mock.calls[1]?.[1] as RequestInit).body as string) as Record<string, string>;
  expect(body).toEqual({ email: "ada@example.test", password: "wrong-password" });
});
