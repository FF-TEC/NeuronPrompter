/**
 * Typed HTTP client for the NeuronPrompter REST API. Wraps the native fetch API
 * with JSON serialization, error handling, retry logic, and typed request/response
 * pairs.
 *
 * All paths are relative to /api/v1 and are proxied to the Rust backend during
 * development via Vite's dev server proxy configuration.
 */

import type {
  AppSetting,
  Chain,
  ChainFilter,
  ChainWithSteps,
  ClipboardEntry,
  CopyResult,
  CopySummary,
  Category,
  Collection,
  DbPathResponse,
  DerivedMetadata,
  DoctorProbes,
  HealthResponse,
  ImportSummary,
  McpStatusResponse,
  McpTarget,
  NewChain,
  NewPrompt,
  NewScript,
  OllamaCatalogResponse,
  OllamaDeleteResponse,
  OllamaModelsResponse,
  OllamaPullResponse,
  OllamaRunningResponse,
  OllamaShowResponse,
  OllamaStatus,
  OllamaStatusResponse,
  PaginatedList,
  Prompt,
  PromptFilter,
  PromptVersion,
  PromptWithAssociations,
  Script,
  ScriptFilter,
  ScriptVersion,
  ScriptWithAssociations,
  SessionMeResponse,
  SessionResponse,
  SetupStatus,
  SyncReport,
  Tag,
  UpdateChain,
  UpdatePrompt,
  UpdateScript,
  User,
  UserSettings,
} from "./types";

/** Base path for all API requests. */
const BASE = "/api/v1";

/** Default timeout in milliseconds for standard API requests. */
const DEFAULT_TIMEOUT_MS = 30_000;

/** Timeout for long-running operations (Ollama inference, import/export). */
const LONG_TIMEOUT_MS = 120_000;

/** Error thrown when the API returns a non-2xx status code. */
export class ApiError extends Error {
  constructor(
    public status: number,
    public body: string,
  ) {
    super(`API error ${status}: ${body}`);
    this.name = "ApiError";
  }
}

/**
 * Wraps fetch() with an AbortController-based timeout.
 */
async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<Response> {
  const controller = new AbortController();
  const timer = timeoutMs > 0
    ? setTimeout(() => controller.abort(), timeoutMs)
    : undefined;

  let forwardAbort: (() => void) | undefined;
  const existing = init.signal;

  try {
    if (existing) {
      forwardAbort = () => controller.abort();
      existing.addEventListener("abort", forwardAbort);
    }
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    if (existing && forwardAbort) {
      existing.removeEventListener("abort", forwardAbort);
    }
  }
}

/** HTTP status codes for which a request may be retried. */
const RETRYABLE_STATUSES = new Set([408, 429, 500, 502, 503, 504]);

/** Retryable statuses for non-idempotent methods (excludes 500 to avoid duplicate side-effects). */
const RETRYABLE_STATUSES_NON_IDEMPOTENT = new Set([408, 429, 502, 503, 504]);

/**
 * Wraps a single fetch attempt with automatic retry on retryable HTTP status codes.
 * Only retries 500 errors for idempotent methods (GET, HEAD, OPTIONS).
 */
async function fetchWithRetry(
  url: string,
  init: RequestInit,
  timeoutMs: number,
  maxRetries = 2,
  baseDelayMs = 500,
): Promise<Response> {
  const method = (init.method ?? "GET").toUpperCase();
  // DELETE is classified as non-idempotent here as a conservative safety measure.
  // While HTTP DELETE is semantically idempotent, retrying a failed DELETE that
  // actually succeeded server-side could produce confusing 404 errors or, in edge
  // cases, delete a different resource that was created with the same ID.
  const isIdempotent = !["POST", "DELETE"].includes(method);
  const retryableForMethod = isIdempotent ? RETRYABLE_STATUSES : RETRYABLE_STATUSES_NON_IDEMPOTENT;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    const res = await fetchWithTimeout(url, init, timeoutMs);
    if (!retryableForMethod.has(res.status) || attempt === maxRetries) return res;
    const retryAfterHeader = res.headers.get("Retry-After");
    const parsed = retryAfterHeader ? Number(retryAfterHeader) : NaN;
    const delay = !isNaN(parsed) && parsed > 0
      ? parsed * 1000
      : baseDelayMs * Math.pow(2, attempt);
    await new Promise<void>((r) => setTimeout(r, delay));
  }
  throw new Error("retry loop exhausted");
}

/**
 * Parses the JSON body from a successful response.
 * Throws on empty/null bodies to surface unexpected empty replies (AUD-084).
 * Use `parseVoidBody` for endpoints that legitimately return no content.
 */
async function parseJsonBody<T>(res: Response): Promise<T> {
  const text = await res.text();
  if (!text || text === "null") {
    throw new Error("Unexpected empty response body");
  }
  return JSON.parse(text) as T;
}

/**
 * Consumes (but does not parse) the body of a response from a void-returning
 * endpoint. Accepts 204 No Content and empty bodies without error.
 */
async function parseVoidBody(res: Response): Promise<void> {
  // Drain the body to release the connection, but discard the content.
  await res.text();
}

/**
 * Sends a GET request to the given API path and returns parsed JSON.
 */
async function get<T>(path: string, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<T> {
  const res = await fetchWithRetry(`${BASE}${path}`, {}, timeoutMs);
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  return parseJsonBody<T>(res);
}

/**
 * Sends a POST request with a JSON body.
 */
async function post<T>(path: string, body?: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<T> {
  const res = await fetchWithRetry(
    `${BASE}${path}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    },
    timeoutMs,
  );
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  return parseJsonBody<T>(res);
}

/**
 * Sends a PUT request with a JSON body.
 */
async function put<T>(path: string, body?: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<T> {
  const res = await fetchWithRetry(
    `${BASE}${path}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    },
    timeoutMs,
  );
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  return parseJsonBody<T>(res);
}

// ---- Void variants (for endpoints that return no body) ----

async function postVoid(path: string, body?: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<void> {
  const res = await fetchWithRetry(
    `${BASE}${path}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    },
    timeoutMs,
  );
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  await parseVoidBody(res);
}

async function putVoid(path: string, body?: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<void> {
  const res = await fetchWithRetry(
    `${BASE}${path}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    },
    timeoutMs,
  );
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  await parseVoidBody(res);
}

async function patchVoid(path: string, body?: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<void> {
  const res = await fetchWithRetry(
    `${BASE}${path}`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    },
    timeoutMs,
  );
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  await parseVoidBody(res);
}

async function delVoid(path: string, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<void> {
  const res = await fetchWithRetry(`${BASE}${path}`, { method: "DELETE" }, timeoutMs);
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  await parseVoidBody(res);
}

// =============================================================================
// Clipboard utility
// =============================================================================

/**
 * Writes content to the system clipboard.
 *
 * Uses a two-tier strategy:
 *  1. `navigator.clipboard.writeText` (modern, async, requires secure context)
 *  2. Hidden-textarea + `document.execCommand("copy")` (legacy, synchronous)
 *
 * The legacy fallback is tried FIRST if the Clipboard API is unavailable
 * (e.g. plain HTTP or restricted iframe). Both tiers are attempted so at
 * least one succeeds in all common browser environments.
 *
 * MUST be called from a synchronous click-handler path — no `await` before
 * this call — so the browser still considers the user gesture active.
 */
export function writeToSystemClipboard(content: string): boolean {
  // ---- Tier 1: legacy execCommand (works synchronously, broad support) ----
  let legacyOk = false;
  try {
    const ta = document.createElement("textarea");
    ta.value = content;
    // Place offscreen so it doesn't flash
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.left = "-9999px";
    ta.style.top = "-9999px";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    legacyOk = document.execCommand("copy");
    document.body.removeChild(ta);
  } catch {
    // execCommand not available
  }

  if (legacyOk) return true;

  // ---- Tier 2: modern Clipboard API (async, needs secure context) ----
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(content).catch(() => {
      console.warn("Clipboard API also failed — content was saved to history only");
    });
    // Async API initiated; assume success since we cannot await in a sync path.
    return true;
  }

  return false;
}

/** Extract template variable names from content (client-side, no network needed).
 *  Regex matches the backend Rust regex: only valid identifiers are captured. */
export function extractTemplateVariables(content: string): string[] {
  const vars: string[] = [];
  for (const m of content.matchAll(/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g)) {
    const name = m[1] ?? "";
    if (name && !vars.includes(name)) vars.push(name);
  }
  return vars;
}

// =============================================================================
// Typed API client
// =============================================================================

export const api = {
  // ---- Health ----
  health: () => get<HealthResponse>("/health"),
  getDbPath: () => get<DbPathResponse>("/settings/db-path"),

  // ---- Sessions ----
  createSession: (userId: number) =>
    post<SessionResponse>("/sessions", { user_id: userId }),
  switchSession: (userId: number) =>
    put<SessionResponse>("/sessions/switch", { user_id: userId }),
  logout: () =>
    delVoid("/sessions"),
  sessionMe: async (): Promise<SessionMeResponse | null> => {
    try {
      return await get<SessionMeResponse>("/sessions/me");
    } catch {
      return null;
    }
  },

  // ---- Users ----
  listUsers: () => get<User[]>("/users"),
  createUser: (username: string, displayName: string) =>
    post<User>("/users", { username, display_name: displayName }),
  switchUser: (userId: number) =>
    put<SessionResponse>("/sessions/switch", { user_id: userId }),
  deleteUser: (userId: number) =>
    delVoid(`/users/${userId}`),
  updateUser: (userId: number, displayName: string, username: string) =>
    put<User>(`/users/${userId}`, { display_name: displayName, username }),

  // ---- Prompts ----
  listPrompts: (filter: PromptFilter) =>
    post<PaginatedList<Prompt>>("/prompts/search", filter),
  getPrompt: (promptId: number) =>
    get<PromptWithAssociations>(`/prompts/${promptId}`),
  createPrompt: (payload: NewPrompt) =>
    post<Prompt>("/prompts", payload),
  updatePrompt: (payload: UpdatePrompt) =>
    put<Prompt>(`/prompts/${payload.prompt_id}`, payload),
  deletePrompt: (promptId: number) =>
    delVoid(`/prompts/${promptId}`),
  duplicatePrompt: (promptId: number) =>
    post<Prompt>(`/prompts/${promptId}/duplicate`),
  copyPromptToUser: (promptId: number, targetUserId: number) =>
    post<CopySummary>(`/prompts/${promptId}/copy-to-user`, { target_user_id: targetUserId }),
  toggleFavorite: (promptId: number, isFavorite: boolean) =>
    patchVoid(`/prompts/${promptId}/favorite`, { value: isFavorite }),
  toggleArchive: (promptId: number, isArchived: boolean) =>
    patchVoid(`/prompts/${promptId}/archive`, { value: isArchived }),

  // ---- Tags ----
  listTags: (userId: number) =>
    get<Tag[]>(`/tags/user/${userId}`),
  createTag: (userId: number, name: string) =>
    post<Tag>("/tags", { user_id: userId, name }),
  renameTag: (tagId: number, newName: string) =>
    putVoid(`/tags/${tagId}`, { new_name: newName }),
  deleteTag: (tagId: number) =>
    delVoid(`/tags/${tagId}`),

  // ---- Collections ----
  listCollections: (userId: number) =>
    get<Collection[]>(`/collections/user/${userId}`),
  createCollection: (userId: number, name: string) =>
    post<Collection>("/collections", { user_id: userId, name }),
  renameCollection: (collectionId: number, newName: string) =>
    putVoid(`/collections/${collectionId}`, { new_name: newName }),
  deleteCollection: (collectionId: number) =>
    delVoid(`/collections/${collectionId}`),

  // ---- Categories ----
  listCategories: (userId: number) =>
    get<Category[]>(`/categories/user/${userId}`),
  createCategory: (userId: number, name: string) =>
    post<Category>("/categories", { user_id: userId, name }),
  renameCategory: (categoryId: number, newName: string) =>
    putVoid(`/categories/${categoryId}`, { new_name: newName }),
  deleteCategory: (categoryId: number) =>
    delVoid(`/categories/${categoryId}`),

  // ---- Scripts ----
  listScripts: (filter: ScriptFilter) =>
    post<PaginatedList<Script>>("/scripts/search", filter),
  getScript: (scriptId: number) =>
    get<ScriptWithAssociations>(`/scripts/${scriptId}`),
  createScript: (payload: NewScript) =>
    post<Script>("/scripts", payload),
  updateScript: (payload: UpdateScript) =>
    put<Script>(`/scripts/${payload.script_id}`, payload),
  deleteScript: (scriptId: number) =>
    delVoid(`/scripts/${scriptId}`),
  duplicateScript: (scriptId: number) =>
    post<Script>(`/scripts/${scriptId}/duplicate`),
  copyScriptToUser: (scriptId: number, targetUserId: number) =>
    post<CopySummary>(`/scripts/${scriptId}/copy-to-user`, { target_user_id: targetUserId }),
  toggleScriptFavorite: (scriptId: number, isFavorite: boolean) =>
    patchVoid(`/scripts/${scriptId}/favorite`, { value: isFavorite }),
  toggleScriptArchive: (scriptId: number, isArchived: boolean) =>
    patchVoid(`/scripts/${scriptId}/archive`, { value: isArchived }),
  searchScripts: (userId: number, query: string, filter?: ScriptFilter) =>
    post<Script[]>("/search/scripts", { user_id: userId, query, filter: filter ?? {} }),
  syncScripts: (userId: number) =>
    post<SyncReport>("/scripts/sync", { user_id: userId }),
  importScriptFile: (userId: number, path: string, isSynced: boolean) =>
    post<Script>("/scripts/import-file", { user_id: userId, path, is_synced: isSynced }),

  // ---- Versions ----
  listVersions: (promptId: number) =>
    get<PromptVersion[]>(`/versions/prompt/${promptId}`),
  getVersion: (versionId: number) =>
    get<PromptVersion>(`/versions/${versionId}`),
  restoreVersion: (promptId: number, versionNumber: number) =>
    post<Prompt>(`/versions/prompt/${promptId}/restore`, { version_number: versionNumber }),

  // ---- Script Versions ----
  listScriptVersions: (scriptId: number) =>
    get<ScriptVersion[]>(`/script-versions/script/${scriptId}`),
  getScriptVersion: (versionId: number) =>
    get<ScriptVersion>(`/script-versions/${versionId}`),
  restoreScriptVersion: (scriptId: number, versionNumber: number) =>
    post<Script>(`/script-versions/script/${scriptId}/restore`, { version_number: versionNumber }),

  // ---- Chains ----
  listChains: (filter: ChainFilter) =>
    post<PaginatedList<Chain>>("/chains/search", filter),
  getChain: (chainId: number) =>
    get<ChainWithSteps>(`/chains/${chainId}`),
  createChain: (payload: NewChain) =>
    post<Chain>("/chains", payload),
  updateChain: (payload: UpdateChain) =>
    put<Chain>(`/chains/${payload.chain_id}`, payload),
  deleteChain: (chainId: number) =>
    delVoid(`/chains/${chainId}`),
  duplicateChain: (chainId: number) =>
    post<Chain>(`/chains/${chainId}/duplicate`),
  copyChainToUser: (chainId: number, targetUserId: number) =>
    post<CopySummary>(`/chains/${chainId}/copy-to-user`, { target_user_id: targetUserId }, LONG_TIMEOUT_MS),
  toggleChainFavorite: (chainId: number, isFavorite: boolean) =>
    patchVoid(`/chains/${chainId}/favorite`, { value: isFavorite }),
  toggleChainArchive: (chainId: number, isArchived: boolean) =>
    patchVoid(`/chains/${chainId}/archive`, { value: isArchived }),
  getChainContent: (chainId: number) =>
    get<{ content: string }>(`/chains/${chainId}/content`),
  chainsForPrompt: (promptId: number) =>
    get<Chain[]>(`/chains/by-prompt/${promptId}`),
  searchChains: (userId: number, query: string, filter?: ChainFilter) =>
    post<Chain[]>("/search/chains", { user_id: userId, query, filter: filter ?? {} }),

  // ---- Counts ----
  countPrompts: () =>
    get<{ count: number }>("/prompts/count"),
  countScripts: () =>
    get<{ count: number }>("/scripts/count"),
  countChains: () =>
    get<{ count: number }>("/chains/count"),

  // ---- Languages ----
  listPromptLanguages: () =>
    get<string[]>("/prompts/languages"),
  listScriptLanguages: () =>
    get<string[]>("/scripts/languages"),

  // ---- Search ----
  searchPrompts: (userId: number, query: string, filter?: PromptFilter) =>
    post<Prompt[]>("/search/prompts", { user_id: userId, query, filter: filter ?? {} }),

  // ---- Import/Export ----
  exportJson: (userId: number, promptIds: number[], path: string) =>
    postVoid("/io/export/json", { user_id: userId, prompt_ids: promptIds, path }),
  importJson: (userId: number, path: string) =>
    post<ImportSummary>("/io/import/json", { user_id: userId, path }),
  exportMarkdown: (userId: number, promptIds: number[], dirPath: string) =>
    postVoid("/io/export/markdown", { user_id: userId, prompt_ids: promptIds, dir_path: dirPath }),
  importMarkdown: (userId: number, dirPath: string) =>
    post<ImportSummary>("/io/import/markdown", { user_id: userId, dir_path: dirPath }),
  backupDatabase: (targetPath: string) =>
    postVoid("/io/backup", { target_path: targetPath }),

  // Cross-user bulk copy
  bulkCopyAll: (sourceUserId: number, targetUserId: number) =>
    post<CopySummary>("/users/bulk-copy", { source_user_id: sourceUserId, target_user_id: targetUserId }, LONG_TIMEOUT_MS),

  // ---- Clipboard ----
  copyToClipboard: (content: string, promptTitle: string) =>
    post<CopyResult>("/clipboard/copy", { content, prompt_title: promptTitle }),
  copyWithSubstitution: (content: string, promptTitle: string, values: Record<string, string>) =>
    post<string>("/clipboard/copy-substituted", { content, prompt_title: promptTitle, values }),
  getClipboardHistory: () =>
    get<ClipboardEntry[]>("/clipboard/history"),
  clearClipboardHistory: () =>
    delVoid("/clipboard/history"),

  // ---- Settings ----
  getAppSetting: async (key: string): Promise<AppSetting | null> => {
    const res = await fetchWithRetry(`${BASE}/settings/app/${encodeURIComponent(key)}`, {}, DEFAULT_TIMEOUT_MS);
    if (!res.ok) {
      const text = await res.text();
      throw new ApiError(res.status, text);
    }
    const text = await res.text();
    if (!text || text === "null") return null;
    return JSON.parse(text) as AppSetting;
  },
  setAppSetting: (key: string, value: string) =>
    putVoid(`/settings/app/${encodeURIComponent(key)}`, { value }),
  getUserSettings: (userId: number) =>
    get<UserSettings>(`/settings/user/${userId}`),
  updateUserSettings: (payload: UserSettings) =>
    putVoid("/settings/user", payload),

  // ---- Ollama (core API endpoints) ----
  ollamaStatus: (baseUrl: string) =>
    post<OllamaStatus>("/ollama/status", { base_url: baseUrl }),
  ollamaImprove: (baseUrl: string, model: string, content: string) =>
    post<string>("/ollama/improve", { base_url: baseUrl, model, content }, LONG_TIMEOUT_MS),
  ollamaTranslate: (baseUrl: string, model: string, content: string, targetLanguage: string) =>
    post<string>("/ollama/translate", { base_url: baseUrl, model, content, target_language: targetLanguage }, LONG_TIMEOUT_MS),
  ollamaAutofill: (baseUrl: string, model: string, content: string) =>
    post<DerivedMetadata>("/ollama/autofill", { base_url: baseUrl, model, content }, LONG_TIMEOUT_MS),

  // ---- Web Ollama (web-specific endpoints for model management) ----
  // These endpoints resolve the Ollama URL from the active user's settings
  // in the database -- no URL parameter needed from the frontend.
  ollamaWebStatus: () =>
    get<OllamaStatusResponse>("/web/ollama/status"),
  ollamaModels: () =>
    get<OllamaModelsResponse>("/web/ollama/models"),
  ollamaRunning: () =>
    get<OllamaRunningResponse>("/web/ollama/running"),
  ollamaCatalog: () =>
    get<OllamaCatalogResponse>("/web/ollama/catalog"),
  ollamaPull: (model: string) =>
    post<OllamaPullResponse>("/web/ollama/pull", { model }, LONG_TIMEOUT_MS),
  ollamaDelete: (model: string) =>
    post<OllamaDeleteResponse>("/web/ollama/delete", { model }),
  ollamaShow: (model: string) =>
    post<OllamaShowResponse>("/web/ollama/show", { model }),

  // ---- Setup / Doctor ----
  setupStatus: () =>
    get<SetupStatus>("/web/setup/status"),
  setupComplete: () =>
    postVoid("/web/setup/complete"),
  doctorProbes: () =>
    get<DoctorProbes>("/web/doctor/probes"),

  // ---- MCP ----
  mcpStatus: () =>
    get<McpStatusResponse>("/web/mcp/status"),
  mcpInstall: (target: McpTarget) =>
    post<{ status: string }>(`/web/mcp/${target}/install`),
  mcpUninstall: (target: McpTarget) =>
    post<{ status: string }>(`/web/mcp/${target}/uninstall`),
};
