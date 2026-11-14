// Mirrors the JSON contracts in contracts/openapi.json.

export type Role =
  | "owner"
  | "administrator"
  | "project_manager"
  | "contributor"
  | "reviewer"
  | "billing_administrator"
  | "auditor"
  | "read_only";

export type Permission =
  | "workspace_read"
  | "members_manage"
  | "projects_manage"
  | "documents_write"
  | "documents_approve"
  | "decisions_write"
  | "decisions_decide"
  | "providers_use"
  | "audit_read"
  | "usage_read"
  | "billing_manage";

export const ROLE_LABELS: Record<Role, string> = {
  owner: "Owner",
  administrator: "Administrator",
  project_manager: "Project manager",
  contributor: "Contributor",
  reviewer: "Reviewer",
  billing_administrator: "Billing administrator",
  auditor: "Auditor",
  read_only: "Read-only",
};

export interface Membership {
  organization_id: string;
  organization_name: string;
  role: Role;
  permissions: Permission[];
}

export interface SessionView {
  user: { id: string; email: string; display_name: string; created_at: string };
  memberships: Membership[];
  csrf_token: string;
}

export interface Paged<T> {
  items: T[];
  limit: number;
  offset: number;
  has_more: boolean;
}

export type ProjectStatus = "active" | "on_hold" | "completed" | "archived";

export interface Project {
  id: string;
  name: string;
  description: string;
  status: ProjectStatus;
  created_by: string;
  created_at: string;
  updated_at: string;
  document_count: number;
  decision_count: number;
}

export const DOCUMENT_CATEGORIES = {
  software_requirements: "Software requirements specification",
  business_proposal: "Business proposal",
  product_requirements: "Product requirements document",
  architecture_decision_record: "Architecture decision record",
  engineering_report: "Engineering report",
  research_brief: "Research brief",
  project_plan: "Project plan",
  meeting_minutes: "Meeting minutes",
  executive_summary: "Executive summary",
  standard_operating_procedure: "Standard operating procedure",
  general: "General",
} as const;
export type DocumentCategory = keyof typeof DOCUMENT_CATEGORIES;

export interface DocumentSummary {
  id: string;
  project_id: string;
  title: string;
  category: DocumentCategory;
  status: "draft" | "approved";
  current_version: number;
  approved_version: number | null;
  created_at: string;
  updated_at: string;
}

export interface DocumentVersion {
  version: number;
  title: string;
  body: string;
  ai_assisted: boolean;
  change_note: string;
  created_by: string;
  created_by_name: string;
  created_at: string;
}

export interface DocumentDetail extends DocumentSummary {
  current: DocumentVersion;
}

export interface VersionSummary {
  version: number;
  title: string;
  ai_assisted: boolean;
  change_note: string;
  created_by_name: string;
  created_at: string;
  body_chars: number;
}

export type DecisionStatus = "draft" | "under_review" | "approved" | "rejected" | "superseded";

export interface DecisionContent {
  title: string;
  problem_statement: string;
  context: string;
  alternatives: string;
  evaluation_criteria: string;
  evidence: string;
  outcome: string;
  consequences: string;
  risks: string;
}

export interface Decision extends DecisionContent {
  id: string;
  project_id: string;
  reference: string;
  number: number;
  decision_maker_id: string | null;
  decision_maker_name: string | null;
  review_date: string | null;
  status: DecisionStatus;
  superseded_by: string | null;
  version: number;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface DecisionVersion {
  version: number;
  status: DecisionStatus;
  snapshot: Partial<DecisionContent> & { status?: string };
  note: string;
  changed_by_name: string;
  changed_at: string;
}

export interface Member {
  user_id: string;
  email: string;
  display_name: string;
  role: Role;
  joined_at: string;
}

export interface Invitation {
  id: string;
  email: string;
  role: Role;
  status: "pending" | "accepted" | "revoked" | "expired";
  created_at: string;
  expires_at: string;
}

export type ProviderId = "openai" | "anthropic" | "gemini" | "xai";
export type ProviderStatus = "disabled" | "not_configured" | "configured";
export type Task = "planning" | "requirements" | "research" | "review";
export type Mode = "fixture" | "live";

export interface ProviderInfo {
  id: ProviderId;
  vendor: string;
  product: string;
  perspective: string;
  status: ProviderStatus;
  model: string | null;
  capabilities: string[];
  data_handling: string;
  fixture_available: boolean;
}

export interface Catalog {
  providers: ProviderInfo[];
  tasks: { id: Task; label: string; instruction: string }[];
  prompt_max_chars: number;
  live_enabled: boolean;
}

export interface RunResult {
  request_id: string;
  provider: ProviderId;
  model: string;
  mode: Mode;
  text: string;
  usage: { input_tokens: number | null; output_tokens: number | null } | null;
  latency_ms: number;
  truncated: boolean;
  notice: string;
}

export interface UsageRecord {
  id: string;
  user_name: string;
  project_id: string | null;
  provider: ProviderId;
  model: string;
  task: Task;
  mode: Mode;
  outcome: "succeeded" | "failed";
  prompt_chars: number;
  input_tokens: number | null;
  output_tokens: number | null;
  latency_ms: number;
  created_at: string;
}

export interface AuditEvent {
  id: string;
  actor_user_id: string | null;
  actor_name: string | null;
  event_type: string;
  resource_type: string | null;
  resource_id: string | null;
  outcome: "success" | "denied" | "failure";
  correlation_id: string;
  details: Record<string, unknown>;
  occurred_at: string;
}
