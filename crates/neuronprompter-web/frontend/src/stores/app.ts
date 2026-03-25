/**
 * Central application store for the NeuronPrompter web frontend.
 * Uses SolidJS createStore for fine-grained reactivity at the property level.
 */

import { createStore, produce, reconcile } from "solid-js/store";

import type {
  Category,
  Chain,
  ChainWithSteps,
  Collection,
  ClipboardEntry,
  NavFilter,
  OllamaModelDto,
  Prompt,
  PromptWithAssociations,
  Script,
  ScriptWithAssociations,
  Tag,
  User,
  UserSettings,
} from "../api/types";

// ---------------------------------------------------------------------------
// Store shape
// ---------------------------------------------------------------------------

/** Tab identifiers for the main navigation. */
export type AppTab = "prompts" | "organize" | "chains" | "scripts" | "models" | "clipboard" | "settings" | "users";

/** Structured log entry received from the backend via SSE. */
export interface LogEntry {
  level: string;
  target: string;
  message: string;
  timestamp: string;
}

/** Toast notification entry. */
export interface ToastMessage {
  id: number;
  type: "success" | "error" | "info";
  title: string;
  message: string;
}

/** Application state. */
export interface AppState {
  // ---- Navigation ----
  activeTab: AppTab;
  logPanelOpen: boolean;

  // ---- Users ----
  users: User[];
  activeUser: User | null;

  // ---- Prompts ----
  prompts: Prompt[];
  activePromptId: number | null;
  activePromptDetail: PromptWithAssociations | null;
  editorDirty: boolean;

  // ---- Chains ----
  chains: Chain[];
  activeChainId: number | null;
  activeChainDetail: ChainWithSteps | null;
  chainEditorDirty: boolean;

  // ---- Scripts ----
  scripts: Script[];
  activeScriptId: number | null;
  activeScriptDetail: ScriptWithAssociations | null;
  scriptEditorDirty: boolean;

  // ---- Navigation filter ----
  navFilter: NavFilter;

  // ---- Taxonomy ----
  tags: Tag[];
  categories: Category[];
  collections: Collection[];

  // ---- Search ----
  searchQuery: string;

  // ---- Clipboard ----
  clipboardHistory: ClipboardEntry[];

  // ---- Toasts ----
  toasts: ToastMessage[];

  // ---- Logs ----
  logMessages: LogEntry[];

  // ---- Ollama ----
  ollamaUrl: string;
  ollamaModel: string | null;
  ollamaConnected: boolean;
  ollamaModels: OllamaModelDto[];
  ollamaRunningModels: string[];
  ollamaPullingModel: string | null;
  ollamaPullProgress: { total: number; completed: number } | null;

  // ---- MCP ----
  mcpClaudeCode: boolean;
  mcpClaudeDesktop: boolean;

  // ---- Settings ----
  userSettings: UserSettings | null;

  // ---- Modals ----
  newUserModalOpen: boolean;
  deleteModalOpen: boolean;
  templateDialogOpen: boolean;
  templateVariables: string[];
  templateContent: string;
  templatePromptTitle: string;

  // ---- Save handler ----
  saveHandler: (() => Promise<void>) | null;

  // ---- Unsaved changes dialog ----
  unsavedDialogOpen: boolean;
  unsavedDialogCallbacks: {
    onSave: () => void;
    onDiscard: () => void;
    onCancel: () => void;
  } | null;

  // ---- App metadata ----
  appVersion: string | null;
  dbPath: string | null;

  // ---- Loading ----
  loading: boolean;
  loadError: string | null;
}

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

const initialState: AppState = {
  activeTab: "prompts",
  logPanelOpen: false,

  users: [],
  activeUser: null,

  prompts: [],
  activePromptId: null,
  activePromptDetail: null,
  editorDirty: false,

  chains: [],
  activeChainId: null,
  activeChainDetail: null,
  chainEditorDirty: false,

  scripts: [],
  activeScriptId: null,
  activeScriptDetail: null,
  scriptEditorDirty: false,

  navFilter: { kind: "all" },

  tags: [],
  categories: [],
  collections: [],

  searchQuery: "",

  clipboardHistory: [],

  toasts: [],

  logMessages: [],

  ollamaUrl: "http://localhost:11434",
  ollamaModel: null,
  ollamaConnected: false,
  ollamaModels: [],
  ollamaRunningModels: [],
  ollamaPullingModel: null,
  ollamaPullProgress: null,

  mcpClaudeCode: false,
  mcpClaudeDesktop: false,

  userSettings: null,

  newUserModalOpen: false,
  deleteModalOpen: false,
  templateDialogOpen: false,
  templateVariables: [],
  templateContent: "",
  templatePromptTitle: "",

  saveHandler: null,

  unsavedDialogOpen: false,
  unsavedDialogCallbacks: null,

  appVersion: null,
  dbPath: null,

  loading: true,
  loadError: null,
};

// ---------------------------------------------------------------------------
// Store instance
// ---------------------------------------------------------------------------

const MAX_LOG_MESSAGES = 500;
let nextToastId = 1;

/**
 * L-52: Tracks active auto-dismiss timers for toast notifications.
 * Each entry maps a toast ID to its setTimeout handle. When a toast is
 * dismissed (either manually or by auto-dismiss), the timer is cleared
 * and removed from this map to prevent stale timeout callbacks from
 * attempting to remove already-dismissed toasts.
 */
const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

const [state, setState] = createStore<AppState>(initialState);

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

const actions = {
  // ---- Navigation ----
  setActiveTab(tab: AppTab) {
    setState("activeTab", tab);
  },
  toggleLogPanel() {
    setState("logPanelOpen", (prev) => !prev);
  },

  // ---- Users ----
  setUsers(users: User[]) {
    setState("users", users);
  },
  setActiveUser(user: User | null) {
    // Store a plain snapshot to avoid keeping a reactive proxy reference
    // that could change when the users array is updated.
    setState("activeUser", user ? { ...user } : null);
  },

  // ---- Prompts ----
  setPrompts(prompts: Prompt[]) {
    setState("prompts", reconcile(prompts));
  },
  setActivePromptId(id: number | null) {
    setState("activePromptId", id);
  },
  setActivePromptDetail(detail: PromptWithAssociations | null) {
    setState("activePromptDetail", reconcile(detail));
  },
  setEditorDirty(dirty: boolean) {
    setState("editorDirty", dirty);
  },

  // ---- Chains ----
  setChains(chains: Chain[]) {
    setState("chains", reconcile(chains));
  },
  setActiveChainId(id: number | null) {
    setState("activeChainId", id);
  },
  setActiveChainDetail(detail: ChainWithSteps | null) {
    setState("activeChainDetail", reconcile(detail));
  },
  setChainEditorDirty(dirty: boolean) {
    setState("chainEditorDirty", dirty);
  },

  // ---- Scripts ----
  setScripts(scripts: Script[]) {
    setState("scripts", reconcile(scripts));
  },
  setActiveScriptId(id: number | null) {
    setState("activeScriptId", id);
  },
  setActiveScriptDetail(detail: ScriptWithAssociations | null) {
    setState("activeScriptDetail", reconcile(detail));
  },
  setScriptEditorDirty(dirty: boolean) {
    setState("scriptEditorDirty", dirty);
  },

  // ---- Navigation filter ----
  setNavFilter(filter: NavFilter) {
    setState("navFilter", filter);
  },

  // ---- Taxonomy ----
  setTags(tags: Tag[]) {
    setState("tags", reconcile(tags));
  },
  setCategories(categories: Category[]) {
    setState("categories", reconcile(categories));
  },
  setCollections(collections: Collection[]) {
    setState("collections", reconcile(collections));
  },

  // ---- Search ----
  setSearchQuery(query: string) {
    setState("searchQuery", query);
  },

  // ---- Clipboard ----
  setClipboardHistory(entries: ClipboardEntry[]) {
    setState("clipboardHistory", entries);
  },

  // ---- Toasts ----

  /**
   * Adds a toast notification and schedules its automatic dismissal after 4 seconds.
   * The auto-dismiss timer is tracked in toastTimers so it can be cleared if the
   * toast is dismissed manually before the timeout fires.
   */
  addToast(type: ToastMessage["type"], title: string, message: string) {
    const id = nextToastId++;
    setState(
      produce((s) => {
        s.toasts.unshift({ id, type, title, message });
      }),
    );
    const timerId = setTimeout(() => {
      toastTimers.delete(id);
      setState("toasts", (list) => list.filter((t) => t.id !== id));
    }, 4000);
    toastTimers.set(id, timerId);
  },

  /**
   * Dismisses a toast by ID. Clears the associated auto-dismiss timer if one
   * exists, preventing the stale callback from firing after manual dismissal.
   */
  dismissToast(id: number) {
    const timerId = toastTimers.get(id);
    if (timerId !== undefined) {
      clearTimeout(timerId);
      toastTimers.delete(id);
    }
    setState("toasts", (list) => list.filter((t) => t.id !== id));
  },

  // ---- Logs ----
  appendLogMessage(entry: LogEntry) {
    setState(
      produce((s) => {
        s.logMessages.push(entry);
        if (s.logMessages.length > MAX_LOG_MESSAGES) {
          s.logMessages.splice(0, s.logMessages.length - MAX_LOG_MESSAGES);
        }
      }),
    );
  },
  clearLogMessages() {
    setState("logMessages", []);
  },

  // ---- Ollama ----
  setOllamaUrl(url: string) {
    setState("ollamaUrl", url);
  },
  setOllamaModel(model: string | null) {
    setState("ollamaModel", model);
  },
  setOllamaConnected(connected: boolean) {
    setState("ollamaConnected", connected);
  },
  setOllamaModels(models: OllamaModelDto[]) {
    setState("ollamaModels", models);
  },
  setOllamaRunningModels(names: string[]) {
    setState("ollamaRunningModels", names);
  },
  setOllamaPullingModel(model: string | null) {
    setState("ollamaPullingModel", model);
  },
  setOllamaPullProgress(progress: { total: number; completed: number } | null) {
    setState("ollamaPullProgress", progress);
  },

  // ---- MCP ----
  setMcpClaudeCode(registered: boolean) {
    setState("mcpClaudeCode", registered);
  },
  setMcpClaudeDesktop(registered: boolean) {
    setState("mcpClaudeDesktop", registered);
  },

  // ---- Settings ----
  setUserSettings(settings: UserSettings | null) {
    setState("userSettings", settings);
  },

  // ---- App metadata ----
  setAppVersion(version: string) {
    setState("appVersion", version);
  },
  setDbPath(path: string) {
    setState("dbPath", path);
  },

  // ---- Modals ----
  setNewUserModalOpen(open: boolean) {
    setState("newUserModalOpen", open);
  },
  setDeleteModalOpen(open: boolean) {
    setState("deleteModalOpen", open);
  },
  setTemplateDialogOpen(open: boolean) {
    setState("templateDialogOpen", open);
  },
  openTemplateDialog(variables: string[], content: string, promptTitle: string) {
    setState("templateVariables", variables);
    setState("templateContent", content);
    setState("templatePromptTitle", promptTitle);
    setState("templateDialogOpen", true);
  },

  // ---- Save handler ----
  setSaveHandler(handler: (() => Promise<void>) | null) {
    setState("saveHandler", () => handler);
  },

  // ---- Unsaved changes dialog ----
  openUnsavedDialog(callbacks: { onSave: () => void; onDiscard: () => void; onCancel: () => void }) {
    setState("unsavedDialogCallbacks", () => callbacks);
    setState("unsavedDialogOpen", true);
  },
  closeUnsavedDialog() {
    setState("unsavedDialogOpen", false);
    setState("unsavedDialogCallbacks", null);
  },

  // ---- Loading ----
  setLoading(loading: boolean) {
    setState("loading", loading);
  },
  setLoadError(error: string | null) {
    setState("loadError", error);
  },
};

// ---------------------------------------------------------------------------
// Theme helpers
// ---------------------------------------------------------------------------

/** Listener cleanup handle for the current system theme watcher. */
let systemThemeCleanup: (() => void) | null = null;

function applyTheme(themeValue: "light" | "dark" | "system"): void {
  // Remove any previous system theme listener.
  if (systemThemeCleanup) {
    systemThemeCleanup();
    systemThemeCleanup = null;
  }

  let resolved = themeValue;
  if (themeValue === "system") {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    resolved = mq.matches ? "dark" : "light";

    // Listen for OS theme changes so the UI updates automatically.
    const handler = (e: MediaQueryListEvent) => {
      document.documentElement.setAttribute("data-theme", e.matches ? "dark" : "light");
    };
    mq.addEventListener("change", handler);
    systemThemeCleanup = () => mq.removeEventListener("change", handler);
  }
  document.documentElement.setAttribute("data-theme", resolved);
}

// ---------------------------------------------------------------------------
// Unsaved-changes navigation guard
// ---------------------------------------------------------------------------

/** Returns true if any editor (prompts, chains, scripts) has unsaved changes. */
function isAnyEditorDirty(): boolean {
  return state.editorDirty || state.chainEditorDirty || state.scriptEditorDirty;
}

/**
 * Checks whether any editor has unsaved changes. If dirty, opens the
 * unsaved-changes dialog and returns a Promise that resolves to the
 * user's choice. If no editor is dirty, resolves to "discard" immediately
 * (proceed without saving).
 */
function guardNavigation(): Promise<"save" | "discard" | "cancel"> {
  if (!isAnyEditorDirty()) {
    return Promise.resolve("discard");
  }
  if (state.unsavedDialogOpen) {
    return Promise.resolve("cancel");
  }
  return new Promise((resolve) => {
    actions.openUnsavedDialog({
      onSave: () => {
        actions.closeUnsavedDialog();
        resolve("save");
      },
      onDiscard: () => {
        actions.closeUnsavedDialog();
        actions.setEditorDirty(false);
        actions.setChainEditorDirty(false);
        actions.setScriptEditorDirty(false);
        resolve("discard");
      },
      onCancel: () => {
        actions.closeUnsavedDialog();
        resolve("cancel");
      },
    });
  });
}

export { state, actions, applyTheme, isAnyEditorDirty, guardNavigation };
