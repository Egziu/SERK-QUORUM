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

// ---------------------------------------------------------------- QUORUM Connect

export type Classification = "public" | "internal" | "confidential" | "restricted";
export const CLASSIFICATION_LABELS: Record<Classification, string> = {
  public: "Public",
  internal: "Internal",
  confidential: "Confidential",
  restricted: "Restricted",
};
export type BriefMode = "quick" | "professional";

export interface ConnectProvider {
  id: ProviderId;
  vendor: string;
  web_product: string;
  website_url: string;
  web_status: "available";
  api_status: ProviderStatus;
  api_model: string | null;
  data_handling: string;
}

export interface RoleTemplate {
  key: string;
  name: string;
  focus: string;
  guidance: string;
  built_in: boolean;
  suggested_provider: ProviderId | null;
}

export interface ConnectCatalog {
  providers: ConnectProvider[];
  roles: RoleTemplate[];
  classifications: Classification[];
  external_share_limit: Classification;
  notice: string;
}

export interface BriefContent {
  request: string;
  current_situation: string;
  desired_outcome: string;
  background: string;
  constraints: string;
  requirements: string;
  deliverable_format: string;
  evaluation_criteria: string;
  open_questions: string;
  language: string;
  audience: string;
}

export const EMPTY_BRIEF: BriefContent = {
  request: "",
  current_situation: "",
  desired_outcome: "",
  background: "",
  constraints: "",
  requirements: "",
  deliverable_format: "",
  evaluation_criteria: "",
  open_questions: "",
  language: "",
  audience: "",
};

export interface Brief {
  id: string;
  project_id: string;
  title: string;
  mode: BriefMode;
  classification: Classification;
  version: number;
  content: Partial<BriefContent>;
  created_at: string;
  updated_at: string;
}

export interface PreparedInstruction {
  provider: ProviderId;
  role: string;
  role_name: string;
  instruction: string;
  characters: number;
  estimated_tokens: number;
  sha256: string;
}

export interface ContextItemView {
  kind: "document" | "decision";
  id: string;
  version: number;
  title: string;
  characters: number;
}

export interface Preview {
  brief_version: number;
  classification: Classification;
  external_share_limit: Classification;
  sharing_allowed: boolean;
  context_items: ContextItemView[];
  instructions: PreparedInstruction[];
}

export type HandoffStatus = "prepared" | "opened" | "imported";

export interface Handoff {
  id: string;
  brief_id: string;
  brief_version: number;
  provider: ProviderId;
  role_name: string;
  mode: "web" | "api";
  instruction: string;
  instruction_sha256: string;
  context_items: { kind: string; id: string; version: number; title: string }[];
  status: HandoffStatus;
  website_url: string | null;
  opened_at: string | null;
  created_at: string;
}

export type ReviewStatus = "unreviewed" | "accepted" | "rejected";

export interface AiResponse {
  id: string;
  project_id: string;
  handoff_id: string | null;
  brief_id: string | null;
  brief_version: number | null;
  provider: ProviderId | "other";
  model: string;
  role_name: string | null;
  source_mode: "web_import" | "file_import" | "api" | "fixture";
  provenance: "user_declared" | "provider_reported";
  provenance_label: string;
  body: string;
  characters: number;
  content_sha256: string;
  reference_url: string;
  review_status: ReviewStatus;
  imported_by_name: string;
  imported_at: string;
}

// ---------------------------------------------------------------- Council

export type CouncilStatus =
  | "draft"
  | "awaiting_responses"
  | "ready_for_review"
  | "under_review"
  | "decision_recorded"
  | "archived";

export type AnnotationKind =
  | "note"
  | "highlight"
  | "evidence"
  | "assumption"
  | "contradiction"
  | "alternative"
  | "question"
  | "risk";
export const ANNOTATION_KINDS: AnnotationKind[] = [
  "note",
  "highlight",
  "evidence",
  "assumption",
  "contradiction",
  "alternative",
  "question",
  "risk",
];

export interface CouncilSummary {
  id: string;
  project_id: string;
  title: string;
  question: string;
  status: CouncilStatus;
  response_count: number;
  decision_id: string | null;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface CouncilDetail extends CouncilSummary {
  decision_summary: string;
  decision: { id: string; reference: string; status: DecisionStatus } | null;
  members: { position: number; response: AiResponse; added_at: string }[];
  retired: { position: number; response_id: string; replaced_by: string | null; removed_at: string | null }[];
  annotations: {
    id: string;
    response_id: string | null;
    kind: AnnotationKind;
    quote: string;
    body: string;
    author_id: string;
    author_name: string;
    created_at: string;
  }[];
  ratings: {
    response_id: string;
    criterion: string;
    score: number;
    reason: string;
    rated_by: string;
    rated_by_name: string;
  }[];
  criteria: { key: string; label: string }[];
  can_edit: boolean;
  can_decide: boolean;
}
