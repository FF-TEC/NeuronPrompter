/**
 * TypeScript interfaces matching the Rust domain model types from
 * neuronprompter-core. Each interface corresponds to a Serialize/Deserialize
 * struct and matches the JSON shape produced by serde serialization.
 */

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

/** Paginated list wrapper with metadata */
export interface PaginatedList<T> {
  items: T[];
  total: number;
  has_more: boolean;
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

export interface User {
  id: number;
  username: string;
  display_name: string;
  /** Omitted in the public user list endpoint (unauthenticated). */
  created_at?: string;
  /** Omitted in the public user list endpoint (unauthenticated). */
  updated_at?: string;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

/** Default content for newly created prompts (must be non-empty to pass backend validation). */
export const DEFAULT_PROMPT_CONTENT = "Enter your prompt here...";

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

export interface Prompt {
  id: number;
  user_id: number;
  title: string;

  content: string;
  description: string | null;
  notes: string | null;
  language: string | null;
  is_favorite: boolean;
  is_archived: boolean;
  current_version: number;
  created_at: string;
  updated_at: string;
}

export interface PromptWithAssociations {
  prompt: Prompt;
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
}

export interface NewPrompt {
  user_id: number;
  title: string;
  content: string;

  description?: string | null;
  notes?: string | null;
  language?: string | null;
  tag_ids: number[];
  category_ids: number[];
  collection_ids: number[];
}

export interface UpdatePrompt {
  prompt_id: number;
  title?: string;

  content?: string;
  description?: string | null;
  notes?: string | null;
  language?: string | null;
  tag_ids?: number[];
  category_ids?: number[];
  collection_ids?: number[];
}

export interface PromptFilter {
  user_id?: number;
  is_favorite?: boolean;
  is_archived?: boolean;
  collection_id?: number;
  category_id?: number;
  tag_id?: number;
  has_variables?: boolean;
  variable_name?: string;
  limit?: number;
  offset?: number;
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

export interface Tag {
  id: number;
  user_id: number;
  name: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

export interface Collection {
  id: number;
  user_id: number;
  name: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

export interface Category {
  id: number;
  user_id: number;
  name: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

export interface PromptVersion {
  id: number;
  prompt_id: number;
  version_number: number;
  title: string;

  content: string;
  description: string | null;
  notes: string | null;
  language: string | null;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

export interface UserSettings {
  user_id: number;
  theme: "light" | "dark" | "system";
  last_collection_id: number | null;
  sidebar_collapsed: boolean;
  sort_field: "updated_at" | "created_at" | "title";
  sort_direction: "asc" | "desc";
  ollama_base_url: string;
  ollama_model: string | null;
  extra: string;
}

export interface AppSetting {
  key: string;
  value: string;
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

export interface CopyResult {
  copied: boolean;
  variables: string[];
}

export interface ClipboardEntry {
  content: string;
  prompt_title: string;
  copied_at: string;
}

// ---------------------------------------------------------------------------
// Import/Export
// ---------------------------------------------------------------------------

export interface ExportedBy {
  username: string;
  display_name: string;
}

export interface ImportSummary {
  source_user: ExportedBy | null;
  prompts_imported: number;
  tags_created: number;
  categories_created: number;
  collections_created: number;
}

// ---------------------------------------------------------------------------
// Ollama
// ---------------------------------------------------------------------------

export interface OllamaStatus {
  connected: boolean;
  models: string[];
}

export interface DerivedMetadata {
  description: string;
  language: string;
  notes: string;
  suggested_tags: string[];
  suggested_categories: string[];
  errors: string[];
}

/** Single model installed on the Ollama server. */
export interface OllamaModelDto {
  name: string;
  size: number | null;
  digest: string | null;
  modified_at: string | null;
  details: Record<string, unknown> | null;
}

/** Model currently loaded in Ollama GPU/CPU RAM. */
export interface OllamaRunningModelDto {
  name: string;
  size: number;
  size_vram: number | null;
  expires_at: string | null;
  parameter_size: string | null;
  family: string | null;
}

/** Entry from the curated Ollama model catalog. */
export interface OllamaCatalogEntry {
  name: string;
  family: string;
  params: string;
  description: string;
}

export interface OllamaStatusResponse {
  connected: boolean;
  url: string;
}

export interface OllamaModelsResponse {
  models: OllamaModelDto[];
}

export interface OllamaRunningResponse {
  models: OllamaRunningModelDto[];
}

export interface OllamaCatalogResponse {
  models: OllamaCatalogEntry[];
}

export interface OllamaPullResponse {
  status: string;
  model: string;
}

export interface OllamaDeleteResponse {
  status: string;
  model: string;
}

export interface OllamaShowResponse {
  model: string;
  family: string | null;
  parameter_size: string | null;
  quantization_level: string | null;
  template: string | null;
  license: string | null;
  url: string;
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

export interface McpTargetStatus {
  registered: boolean;
  config_path: string;
}

export interface McpStatusResponse {
  claude_code: McpTargetStatus;
  claude_desktop: McpTargetStatus;
  server_version: string;
}

export type McpTarget = "claude-code" | "claude-desktop";

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

export interface HealthResponse {
  status: string;
  version: string;
}

export interface DbPathResponse {
  db_path: string;
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

export interface Script {
  id: number;
  user_id: number;
  title: string;

  content: string;
  description: string | null;
  notes: string | null;
  script_language: string;
  language: string | null;
  is_favorite: boolean;
  is_archived: boolean;
  current_version: number;
  created_at: string;
  updated_at: string;
  source_path: string | null;
  is_synced: boolean;
}

export interface ScriptWithAssociations {
  script: Script;
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
}

export interface NewScript {
  user_id: number;
  title: string;
  content: string;
  script_language: string;

  description?: string | null;
  notes?: string | null;
  language?: string | null;
  source_path?: string | null;
  is_synced?: boolean;
  tag_ids: number[];
  category_ids: number[];
  collection_ids: number[];
}

export interface UpdateScript {
  script_id: number;
  title?: string;

  content?: string;
  description?: string | null;
  notes?: string | null;
  script_language?: string;
  language?: string | null;
  source_path?: string | null;
  is_synced?: boolean;
  tag_ids?: number[];
  category_ids?: number[];
  collection_ids?: number[];
}

export interface SyncReport {
  updated: number;
  unchanged: number;
  errors: SyncError[];
}

export interface SyncError {
  script_id: number;
  title: string;
  source_path: string;
  message: string;
}

export interface ScriptFilter {
  user_id?: number;
  is_favorite?: boolean;
  is_archived?: boolean;
  is_synced?: boolean;
  collection_id?: number;
  category_id?: number;
  tag_id?: number;
  limit?: number;
  offset?: number;
}

export interface ScriptVersion {
  id: number;
  script_id: number;
  version_number: number;
  title: string;

  content: string;
  description: string | null;
  notes: string | null;
  script_language: string;
  language: string | null;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Chains
// ---------------------------------------------------------------------------

export interface Chain {
  id: number;
  user_id: number;
  title: string;

  description: string | null;
  notes: string | null;
  language: string | null;
  separator: string;
  is_favorite: boolean;
  is_archived: boolean;
  created_at: string;
  updated_at: string;
}

export interface ChainStepInput {
  step_type: "prompt" | "script";
  item_id: number;
}

export interface ChainStep {
  id: number;
  chain_id: number;
  step_type: string;
  prompt_id: number | null;
  script_id: number | null;
  position: number;
}

export interface ResolvedChainStep {
  step: ChainStep;
  prompt: Prompt | null;
  script: Script | null;
}

export interface ChainWithSteps {
  chain: Chain;
  steps: ResolvedChainStep[];
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
}

export interface NewChain {
  user_id: number;
  title: string;

  description?: string | null;
  notes?: string | null;
  language?: string | null;
  separator?: string;
  prompt_ids: number[];
  steps?: ChainStepInput[];
  tag_ids: number[];
  category_ids: number[];
  collection_ids: number[];
}

export interface UpdateChain {
  chain_id: number;
  title?: string;

  description?: string | null;
  notes?: string | null;
  language?: string | null;
  separator?: string;
  prompt_ids?: number[];
  steps?: ChainStepInput[];
  tag_ids?: number[];
  category_ids?: number[];
  collection_ids?: number[];
}

export interface ChainFilter {
  user_id?: number;
  is_favorite?: boolean;
  is_archived?: boolean;
  collection_id?: number;
  category_id?: number;
  tag_id?: number;
  limit?: number;
  offset?: number;
}

// ---------------------------------------------------------------------------
// Cross-User Copy
// ---------------------------------------------------------------------------

export interface CopySummary {
  prompts_copied: number;
  scripts_copied: number;
  chains_copied: number;
  tags_created: number;
  categories_created: number;
  collections_created: number;
  skipped: SkippedItem[];
}

export interface SkippedItem {
  entity_type: string;
  title: string;
  reason: string;
}

export interface UpdateUser {
  display_name: string;
  username: string;
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

export type NavFilter =
  | { kind: "all" }
  | { kind: "favorites" }
  | { kind: "archive" }
  | { kind: "tag"; id: number }
  | { kind: "category"; id: number }
  | { kind: "collection"; id: number };

// ---------------------------------------------------------------------------
// Setup / Doctor
// ---------------------------------------------------------------------------

export interface SetupStatus {
  is_first_run: boolean;
  has_users: boolean;
  data_dir: string | null;
}

export interface DependencyProbe {
  name: string;
  purpose: string;
  available: boolean;
  required: boolean;
  hint: string;
  link: string;
  model_count: number;
}

export interface DoctorProbes {
  probes: DependencyProbe[];
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/** Response from POST /sessions and PUT /sessions/switch */
export interface SessionResponse {
  user: User | null;
  session_created: boolean;
}

/** Response from GET /sessions/me */
export interface SessionMeResponse {
  user: User | null;
  expires_in_secs: number | null;
}
