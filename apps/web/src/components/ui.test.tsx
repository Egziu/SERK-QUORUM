import { render, screen } from "@testing-library/react";
import { ErrorNotice, Field, StatusBadge, humanize } from "./ui";

describe("Field", () => {
  it("associates label, hint and error with the control", () => {
    render(<Field label="Email" hint="Work address" error="Required">{(p) => <input {...p} />}</Field>);
    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("Work address Required");
  });

  it("omits aria-invalid without an error", () => {
    render(<Field label="Name">{(p) => <input {...p} />}</Field>);
    expect(screen.getByLabelText("Name")).not.toHaveAttribute("aria-invalid");
  });
});

describe("StatusBadge", () => {
  it("humanizes status and applies tone", () => {
    render(<StatusBadge status="under_review" />);
    const badge = screen.getByText("Under review");
    expect(badge).toHaveClass("badge", "warn");
  });

  it("uses a custom label", () => {
    render(<StatusBadge status="configured" label="Configured · unverified" />);
    expect(screen.getByText("Configured · unverified")).toHaveClass("ok");
  });
});

describe("ErrorNotice", () => {
  it("announces errors as alerts", () => {
    render(<ErrorNotice message="Boom" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Boom");
  });
});

it("humanize", () => {
  expect(humanize("not_configured")).toBe("Not configured");
});
