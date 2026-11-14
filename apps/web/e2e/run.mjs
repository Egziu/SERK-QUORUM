// Browser acceptance test for the 0.2 vertical slice.
//
// Prerequisites: the API running with QUORUM_ALLOW_SIGNUP=1 and
// QUORUM_WEB_DIR pointing at a fresh `npm run build` (see docs/TESTING_GUIDE.md).
//
//   QUORUM_E2E_URL=http://127.0.0.1:8080 node e2e/run.mjs
//
// Uses synthetic data only. Exits non-zero on any failed step, console
// error, CSP violation or serious/critical axe-core accessibility finding.

import { createRequire } from "node:module";
import { mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const here = dirname(fileURLToPath(import.meta.url));
const axeSource = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");

const BASE = process.env.QUORUM_E2E_URL ?? "http://127.0.0.1:8080";
const SHOTS = process.env.QUORUM_E2E_SCREENSHOTS ?? join(here, "..", "test-results", "screenshots");
mkdirSync(SHOTS, { recursive: true });

const run = Date.now().toString(36);
const email = `e2e-${run}@example.test`;
const password = "synthetic-passphrase-e2e";
const failures = [];
const a11y = [];

function step(name) {
  process.stdout.write(`• ${name}\n`);
}

async function audit(page, label) {
  // Injecting axe as an inline <script> would (correctly) be blocked by the
  // app's CSP. Evaluating the local axe-core file through the DevTools
  // protocol does not weaken the page's policy.
  if (!(await page.evaluate(() => Boolean(window.axe)))) await page.evaluate(axeSource);
  const results = await page.evaluate(async () => {
    const r = await window.axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] },
    });
    return r.violations.map((v) => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.length }));
  });
  const serious = results.filter((v) => v.impact === "serious" || v.impact === "critical");
  a11y.push({ page: label, violations: results });
  if (serious.length) failures.push(`axe ${label}: ${serious.map((v) => `${v.id} (${v.nodes})`).join(", ")}`);
}

async function shot(page, name) {
  await page.screenshot({ path: join(SHOTS, `${name}.png`), fullPage: false });
}

const browser = await chromium.launch();
try {
  const context = await browser.newContext({ viewport: { width: 1360, height: 900 }, deviceScaleFactor: 2, colorScheme: "light" });
  const page = await context.newPage();
  // Browsers log every non-2xx fetch as a console error. HTTP failures are
  // checked precisely below instead; all other console errors fail the run.
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().startsWith("Failed to load resource")) failures.push(`console error: ${m.text()}`);
  });
  page.on("response", (r) => {
    const url = new URL(r.url());
    const expected = r.status() === 401 && url.pathname === "/api/v1/auth/session"; // anonymous session probe
    if (r.status() >= 400 && !expected) failures.push(`HTTP ${r.status()} ${r.request().method()} ${url.pathname}`);
  });
  page.on("pageerror", (e) => failures.push(`page error: ${e.message}`));
  // CSP violations surface as console errors in Chromium; also listen explicitly.
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) => {
      console.error(`CSP violation: ${e.violatedDirective} ${e.blockedURI}`);
    });
  });

  step("unauthenticated visit redirects to sign-in");
  await page.goto(`${BASE}/o/00000000-0000-0000-0000-000000000000`);
  await page.waitForURL(/\/login\?next=/);
  await page.getByRole("button", { name: "Sign in" }).waitFor();
  await audit(page, "login");
  await shot(page, "01-login");

  step("sign up creates an organization");
  await page.getByRole("link", { name: "Create an organization" }).click();
  await page.getByLabel("Your name", { exact: true }).fill("Ada Synthetic");
  await page.getByLabel("Work email", { exact: true }).fill(email);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByLabel("Organization name", { exact: true }).fill("Atlas Engineering (synthetic)");
  await audit(page, "signup");
  await page.getByRole("button", { name: "Create organization" }).click();
  await page.getByRole("heading", { name: "Atlas Engineering (synthetic)" }).waitFor();

  step("create a project");
  await page.getByRole("link", { name: "Projects", exact: true }).click();
  await page.getByRole("button", { name: "New project" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Bridge retrofit");
  await page.getByLabel("Description", { exact: true }).fill("Seismic retrofit study for a synthetic bridge. Objective: choose an isolation strategy.");
  await page.getByRole("button", { name: "Create project" }).click();
  await page.getByRole("heading", { name: "Bridge retrofit" }).waitFor();

  step("create and version a document");
  await page.getByRole("button", { name: "New document" }).click();
  await page.getByLabel("Title", { exact: true }).fill("Retrofit project plan");
  await page.getByRole("button", { name: "Create and edit" }).click();
  await page.getByRole("heading", { name: "Retrofit project plan" }).waitFor();
  await page.getByLabel("Content", { exact: true }).fill("## Milestones\n\n1. Site survey\n2. Option study\n3. Decision");
  await page.getByLabel("Change note", { exact: true }).fill("Initial milestones");
  await page.getByRole("button", { name: "Save as version 2" }).click();
  await page.getByText("version 2", { exact: false }).first().waitFor();
  await page.getByLabel("I have reviewed version 2 and approve it.", { exact: true }).check();
  await page.getByRole("button", { name: "Approve version 2" }).click();
  await page.getByText("approved version 2").waitFor();
  await audit(page, "document");
  await shot(page, "04-document");

  step("record a decision and take it through review");
  await page.getByRole("link", { name: "← Project" }).click();
  await page.getByRole("tab", { name: /Decisions/ }).click();
  await page.getByRole("button", { name: "Record a decision" }).click();
  await page.getByLabel("Title", { exact: true }).fill("Use base isolation bearings");
  await page.getByLabel("Problem statement", { exact: true }).fill("The bridge must meet updated seismic criteria.");
  await page.getByRole("button", { name: "Create draft" }).click();
  await page.getByRole("heading", { name: /DEC-0001/ }).waitFor();
  await page.getByLabel("Alternatives considered", { exact: true }).fill("Base isolation; viscous dampers; strengthening only");
  await page.getByLabel("Decision", { exact: true }).fill("Base isolation bearings at both abutments.");
  await page.getByLabel("Responsible decision maker", { exact: true }).selectOption({ label: "Ada Synthetic" });
  await page.getByRole("button", { name: "Save draft" }).click();
  await page.getByText("Version 2 ·").waitFor();
  await page.getByRole("button", { name: "Submit for review" }).click();
  await page.getByText("Content is frozen while under review").waitFor();
  await page.getByLabel(/I confirm this is a human decision/).check();
  await page.getByRole("button", { name: "Approve" }).click();
  await page.getByText("This record is final").waitFor();
  await audit(page, "decision");
  await shot(page, "05-decision");

  step("project overview lists the document and decision");
  await page.getByRole("link", { name: "← Decision register" }).click();
  await page.getByRole("cell", { name: "DEC-0001" }).waitFor();
  await audit(page, "project");
  await shot(page, "03-project");

  step("Command: fixture request, clearly labelled, saved as draft document");
  await page.getByRole("link", { name: "Command" }).click();
  await page.getByText("Nothing is sent to any AI provider").waitFor();
  await page.getByLabel("Brief", { exact: true }).fill("Outline the review checklist for an isolation bearing submittal.");
  await page.getByRole("button", { name: "Show fixture" }).click();
  await page.getByText("Illustrative fixture — not generated by a live AI model.").waitFor();
  await page.getByLabel("Save to project", { exact: true }).selectOption({ label: "Bridge retrofit" });
  await page.getByRole("button", { name: "Save as draft document" }).click();
  await page.getByRole("link", { name: "Open it for review" }).waitFor();
  await audit(page, "command");
  await shot(page, "06-command");

  step("home summary reflects real records");
  await page.getByRole("link", { name: "Home", exact: true }).click();
  await page.getByText("Documents").first().waitFor();
  await audit(page, "home");
  await shot(page, "02-home");

  step("members, providers & usage, audit records");
  await page.getByRole("link", { name: "Members" }).click();
  await page.getByLabel("Email", { exact: true }).fill(`invitee-${run}@example.test`);
  await page.getByRole("button", { name: "Create invitation" }).click();
  await page.getByText("shown only once").waitFor();
  await audit(page, "members");
  await shot(page, "07-members");
  await page.getByRole("link", { name: "Providers & usage" }).click();
  await page.getByRole("heading", { name: "Usage records" }).waitFor();
  await audit(page, "providers");
  await page.getByRole("link", { name: "Audit records" }).click();
  await page.getByText("decision.status_changed").first().waitFor();
  await audit(page, "audit");
  await shot(page, "08-audit");

  step("dark theme");
  await page.getByLabel("Colour theme", { exact: true }).selectOption("dark");
  await page.getByRole("link", { name: "Projects", exact: true }).click();
  await page.getByRole("link", { name: /Bridge retrofit/ }).click();
  await page.getByRole("heading", { name: "Bridge retrofit" }).waitFor();
  await audit(page, "project-dark");
  await shot(page, "09-project-dark");

  step("mobile layout");
  const mobile = await browser.newContext({ ...require("playwright").devices["iPhone 13"], colorScheme: "light" });
  await mobile.addCookies(await context.cookies());
  const m = await mobile.newPage();
  m.on("pageerror", (e) => failures.push(`mobile page error: ${e.message}`));
  await m.goto(page.url());
  await m.getByRole("heading", { name: "Bridge retrofit" }).waitFor();
  const overflow = await m.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
  if (overflow) failures.push("mobile: horizontal page overflow");
  await m.getByRole("button", { name: "Menu" }).click();
  await m.getByRole("navigation", { name: "Workspace" }).getByRole("link", { name: "Command", exact: true }).waitFor();
  await audit(m, "mobile-menu");
  await shot(m, "10-mobile-menu");
  await mobile.close();

  step("sign out ends the session");
  await page.getByRole("button", { name: "Sign out" }).click();
  await page.waitForURL(/\/login/);
  await page.goto(`${BASE}/o/whatever/projects`);
  await page.waitForURL(/\/login/);
} catch (e) {
  failures.push(`step failed: ${e.message.split("\n")[0]}`);
} finally {
  await browser.close();
}

const total = a11y.reduce((n, p) => n + p.violations.length, 0);
console.log(`\naxe-core: ${a11y.length} page states checked, ${total} violations total`);
for (const p of a11y) for (const v of p.violations) console.log(`  [${v.impact}] ${p.page}: ${v.id} — ${v.help} (${v.nodes})`);
console.log(`screenshots: ${SHOTS}`);
if (failures.length) {
  console.error(`\nFAILED (${failures.length}):\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nE2E PASSED");
