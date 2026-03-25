import { Component, Show, onMount, onCleanup, Switch, Match, createEffect, createSignal, ErrorBoundary, type JSX } from "solid-js";
import { logError } from "./utils/errors";
import { state, actions, applyTheme, guardNavigation, isAnyEditorDirty } from "./stores/app";
import type { AppTab } from "./stores/app";
import { api, writeToSystemClipboard, extractTemplateVariables } from "./api/client";
import { subscribeToLogs, subscribeToModels } from "./api/sse";
import type { PromptFilter, NavFilter } from "./api/types";
import { DEFAULT_PROMPT_CONTENT } from "./api/types";

import Topbar from "./components/layout/Topbar";
import StatusBar from "./components/layout/StatusBar";
import LogPanel from "./components/panels/LogPanel";
import PromptsTab from "./components/tabs/PromptsTab";
import ChainsTab from "./components/tabs/ChainsTab";
import OrganizeTab from "./components/tabs/OrganizeTab";
import ModelsTab from "./components/tabs/ModelsTab";
import ScriptsTab from "./components/tabs/ScriptsTab";
import ClipboardTab from "./components/tabs/ClipboardTab";
import SettingsTab from "./components/tabs/SettingsTab";
import UsersTab from "./components/tabs/UsersTab";
import WelcomeDialog from "./components/ui/WelcomeDialog";
import LoginPage from "./components/LoginPage";
import Toast from "./components/Toast";
import TemplateDialog from "./components/TemplateDialog";
import UnsavedChangesDialog from "./components/UnsavedChangesDialog";
import "./App.css";

/**
 * Root application component. Composes the tab-based layout: Topbar (with
 * tab navigation), active tab content panel, LogPanel, and StatusBar.
 * Orchestrates data loading on startup, SSE subscriptions, keyboard
 * shortcuts, user switching, navigation, and toast management.
 */

const App: Component = () => {
  let cleanupLogs: (() => void) | undefined;
  let cleanupModels: (() => void) | undefined;
  const [showWelcome, setShowWelcome] = createSignal(false);
  const [needsLogin, setNeedsLogin] = createSignal(false);
  const [needsMarkerRepair, setNeedsMarkerRepair] = createSignal(false);
  const tokenRef = { current: 0 };

  /**
   * Counter for selectPrompt calls. Each invocation increments this counter
   * and captures the value; after the async fetch completes, the result is
   * discarded if a subsequent selectPrompt call has already incremented the
   * counter (stale-response guard).
   */
  let selectPromptCounter = 0;

  /**
   * Timer handle for the search debounce. Non-empty search queries are
   * delayed by 250ms so rapid keystrokes do not trigger a request per
   * character.
   */
  let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

  // -------------------------------------------------------------------------
  // Initialization
  // -------------------------------------------------------------------------

  /** Runs the normal app startup: load users, data, SSE, status checks. */
  async function initializeApp(): Promise<void> {
    // Subscribe to SSE streams
    cleanupLogs = subscribeToLogs((entry) => {
      actions.appendLogMessage(entry);
    });

    cleanupModels = subscribeToModels({
      onOllamaPullProgress: (data) => {
        if (data.total !== null && data.completed !== null) {
          actions.setOllamaPullProgress({ total: data.total, completed: data.completed });
        }
      },
      onOllamaPullComplete: () => {
        actions.setOllamaPullingModel(null);
        actions.setOllamaPullProgress(null);
      },
      onOllamaPullError: (data) => {
        actions.setOllamaPullingModel(null);
        actions.setOllamaPullProgress(null);
        logError(`App.ollamaPull.${data.model}`, data.error);
      },
    });

    try {
      // If activeUser is already set (from session login), skip user resolution.
      if (state.activeUser) {
        const userList = await api.listUsers();
        actions.setUsers(userList);
        await loadUserData(state.activeUser.id);
      } else {
        const [userList, lastUserSetting] = await Promise.all([
          api.listUsers(),
          api.getAppSetting("last_user_id"),
        ]);

        actions.setUsers(userList);
        let resolvedUserId: number | null = null;

        if (lastUserSetting && lastUserSetting.value) {
          const id = parseInt(lastUserSetting.value, 10);
          if (!isNaN(id) && userList.some((u) => u.id === id)) {
            resolvedUserId = id;
          }
        }

        if (resolvedUserId === null && userList.length > 0) {
          resolvedUserId = userList[0]!.id;
        }

        if (resolvedUserId !== null) {
          const user = userList.find((u) => u.id === resolvedUserId) ?? null;
          actions.setActiveUser(user);
          if (user) {
            await loadUserData(user.id);
          }
        }
      }

      checkOllamaStatus();
      checkMcpStatus();

      // Fetch application version from the health endpoint.
      try {
        const health = await api.health();
        actions.setAppVersion(health.version);
      } catch {
        // Version display is non-critical; do not block app startup.
      }

      // Fetch database path from authenticated endpoint.
      try {
        const dbInfo = await api.getDbPath();
        actions.setDbPath(dbInfo.db_path);
      } catch {
        // Database path display is non-critical.
      }
    } catch (err) {
      actions.setLoadError(err instanceof Error ? err.message : String(err));
    } finally {
      actions.setLoading(false);
    }
  }

  onMount(async () => {
    // Check if this is a first-run scenario before loading the full app.
    try {
      const setup = await api.setupStatus();
      if (setup.is_first_run) {
        if (setup.has_users) {
          // Marker file missing but users exist -- show login, repair marker after.
          setNeedsMarkerRepair(true);
          setNeedsLogin(true);
        } else {
          setShowWelcome(true);
        }
        actions.setLoading(false);
        return;
      }
    } catch {
      // If the setup endpoint fails, proceed with normal init.
    }

    // Check for existing session.
    const session = await api.sessionMe();
    if (session?.user) {
      // Valid session with user -- initialize app directly.
      actions.setActiveUser(session.user);
      await initializeApp();
    } else {
      // No session or no user selected -- show login.
      setNeedsLogin(true);
      actions.setLoading(false);
    }
  });

  onCleanup(() => {
    cleanupLogs?.();
    cleanupModels?.();
    if (searchDebounceTimer !== undefined) clearTimeout(searchDebounceTimer);
  });

  // -------------------------------------------------------------------------
  // Window close protection
  // -------------------------------------------------------------------------
  createEffect(() => {
    const handler = (e: BeforeUnloadEvent) => {
      if (isAnyEditorDirty()) {
        e.preventDefault();
        e.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", handler);
    onCleanup(() => window.removeEventListener("beforeunload", handler));
  });

  // -------------------------------------------------------------------------
  // Tab switching with unsaved-changes guard
  // -------------------------------------------------------------------------

  /**
   * Switches to the target tab after checking for unsaved editor changes.
   * If the user cancels, the switch is aborted. If the user chooses save,
   * the registered save handler is awaited before switching.
   */
  async function handleTabChange(tab: AppTab): Promise<void> {
    if (tab === state.activeTab) return;
    const result = await guardNavigation();
    if (result === "cancel") return;
    if (result === "save" && state.saveHandler) {
      try {
        await state.saveHandler();
      } catch {
        return;
      }
    }
    actions.setActiveTab(tab);
  }

  // -------------------------------------------------------------------------
  // Keyboard shortcuts
  // -------------------------------------------------------------------------
  createEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;

      if (ctrl && e.key === "n") {
        e.preventDefault();
        if (state.activeTab === "scripts") handleNewScript();
        else if (state.activeTab === "chains") handleNewChain();
        else handleNewPrompt();
      } else if (ctrl && e.key === "s") {
        // L-50: Only prevent browser save dialog when there is a registered
        // save handler that will consume the shortcut.
        if (state.saveHandler) {
          e.preventDefault();
          state.saveHandler();
        }
      } else if (ctrl && e.shiftKey && e.key === "C") {
        e.preventDefault();
        handleCopy();
      } else if (ctrl && e.key === "d") {
        e.preventDefault();
        handleDuplicate();
      } else if (ctrl && e.key === "f") {
        e.preventDefault();
        const searchInput = document.querySelector<HTMLInputElement>(".search-input");
        if (searchInput) searchInput.focus();
      } else if (ctrl && e.key === ",") {
        e.preventDefault();
        handleTabChange(state.activeTab === "settings" ? "prompts" : "settings");
      } else if (ctrl && e.key === "1") {
        e.preventDefault();
        handleTabChange("organize");
      } else if (ctrl && e.key === "2") {
        e.preventDefault();
        handleTabChange("prompts");
      } else if (ctrl && e.key === "3") {
        e.preventDefault();
        handleTabChange("chains");
      } else if (ctrl && e.key === "4") {
        e.preventDefault();
        handleTabChange("scripts");
      } else if (ctrl && e.key === "5") {
        e.preventDefault();
        handleTabChange("clipboard");
      } else if (ctrl && e.key === "6") {
        e.preventDefault();
        handleTabChange("users");
      } else if (e.key === "Escape") {
        if (state.templateDialogOpen) {
          actions.setTemplateDialogOpen(false);
        } else if (state.deleteModalOpen) {
          actions.setDeleteModalOpen(false);
        } else if (state.newUserModalOpen) {
          actions.setNewUserModalOpen(false);
        } else if (state.searchQuery) {
          actions.setSearchQuery("");
          handleSearchChange("");
        }
      }
    };

    window.addEventListener("keydown", handler);
    onCleanup(() => window.removeEventListener("keydown", handler));
  });

  // -------------------------------------------------------------------------
  // Data loading helpers
  // -------------------------------------------------------------------------
  function buildPromptFilter(userId: number, filter: NavFilter): PromptFilter {
    const base: PromptFilter = { user_id: userId };
    switch (filter.kind) {
      case "favorites": return { ...base, is_favorite: true };
      case "archive": return { ...base, is_archived: true };
      case "tag": return { ...base, tag_id: filter.id };
      case "category": return { ...base, category_id: filter.id };
      case "collection": return { ...base, collection_id: filter.id };
      default: return base;
    }
  }

  async function loadUserData(userId: number): Promise<void> {
    // Increment the stale-response token so any in-flight loads are discarded.
    const myToken = ++tokenRef.current;

    // Clear ALL user-scoped state before async calls.
    actions.setActivePromptId(null);
    actions.setActivePromptDetail(null);
    actions.setActiveScriptId(null);
    actions.setActiveScriptDetail(null);
    actions.setActiveChainId(null);
    actions.setActiveChainDetail(null);
    actions.setEditorDirty(false);
    actions.setChainEditorDirty(false);
    actions.setScriptEditorDirty(false);
    actions.setSearchQuery("");
    actions.setNavFilter({ kind: "all" });
    actions.setPrompts([]);
    actions.setScripts([]);
    actions.setChains([]);
    actions.setTags([]);
    actions.setCategories([]);
    actions.setCollections([]);

    const filter = buildPromptFilter(userId, { kind: "all" });

    const [promptPage, tags, categories, collections, chainPage, scriptPage, settings] = await Promise.all([
      api.listPrompts(filter),
      api.listTags(userId),
      api.listCategories(userId),
      api.listCollections(userId),
      api.listChains({ user_id: userId }),
      api.listScripts({ user_id: userId, is_archived: false }),
      api.getUserSettings(userId).catch(() => null),
    ]);

    const promptList = promptPage.items;
    const chainList = chainPage.items;
    const scriptList = scriptPage.items;

    // Stale-response guard: if another loadUserData was called, discard results.
    if (tokenRef.current !== myToken) return;

    actions.setPrompts(promptList);
    actions.setTags(tags);
    actions.setCategories(categories);
    actions.setCollections(collections);
    actions.setChains(chainList);
    actions.setScripts(scriptList);

    // Sync scripts from filesystem (non-blocking)
    api.syncScripts(userId).then(async (syncReport) => {
      if (tokenRef.current !== myToken) return;
      if (syncReport.updated > 0) {
        actions.addToast("info", "Scripts Synced",
          `${syncReport.updated} script(s) updated from filesystem`);
        const refreshed = await api.listScripts({ user_id: userId, is_archived: false });
        if (tokenRef.current !== myToken) return;
        actions.setScripts(refreshed.items);
      }
      if (syncReport.errors.length > 0) {
        actions.addToast("error", "Sync Errors",
          `${syncReport.errors.length} synced file(s) could not be read`);
      }
    }).catch((e) => logError("App.scriptSync", e));

    if (settings) {
      actions.setUserSettings(settings);
      applyTheme(settings.theme);
      actions.setOllamaUrl(settings.ollama_base_url);
      actions.setOllamaModel(settings.ollama_model);
    }

    if (promptList.length > 0) {
      await selectPrompt(promptList[0]!.id);
    }
  }

  async function checkOllamaStatus(): Promise<void> {
    try {
      const status = await api.ollamaStatus(state.ollamaUrl);
      actions.setOllamaConnected(status.connected);
    } catch {
      actions.setOllamaConnected(false);
    }
  }

  async function checkMcpStatus(): Promise<void> {
    try {
      const mcpStatus = await api.mcpStatus();
      actions.setMcpClaudeCode(mcpStatus.claude_code.registered);
      actions.setMcpClaudeDesktop(mcpStatus.claude_desktop.registered);
    } catch {
      // MCP endpoints may not be available yet
    }
  }

  // -------------------------------------------------------------------------
  // User interactions
  // -------------------------------------------------------------------------

  /**
   * Fetches the full prompt detail for the given promptId and sets it as
   * the active prompt. Uses a counter-based stale-response guard so that
   * if selectPrompt is called again before the fetch completes, the earlier
   * response is silently discarded.
   */
  async function selectPrompt(promptId: number): Promise<void> {
    const myCounter = ++selectPromptCounter;
    actions.setActivePromptId(promptId);
    try {
      const detail = await api.getPrompt(promptId);
      // Discard the result if another selectPrompt call has been issued since.
      if (selectPromptCounter !== myCounter) return;
      actions.setActivePromptDetail(detail);
    } catch (err) {
      if (selectPromptCounter !== myCounter) return;
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  /**
   * Handles search query changes from the search input. Empty queries reload
   * the full prompt list for the current filter. Non-empty queries are
   * debounced by 250ms to avoid excessive API calls during rapid typing.
   */
  async function handleSearchChange(query: string): Promise<void> {
    actions.setSearchQuery(query);
    if (!state.activeUser) return;

    // Clear any pending debounce timer from a previous invocation.
    if (searchDebounceTimer !== undefined) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = undefined;
    }

    // Empty queries execute immediately (clear search = restore full list).
    if (!query.trim()) {
      const filter = buildPromptFilter(state.activeUser.id, state.navFilter);
      try {
        const prompts = (await api.listPrompts(filter)).items;
        actions.setPrompts(prompts);
        if (prompts.length > 0) {
          await selectPrompt(prompts[0]!.id);
        }
      } catch (err) {
        actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
      }
      return;
    }

    // Non-empty queries are debounced by 250ms so typing several characters
    // quickly only triggers a single server-side search.
    const userId = state.activeUser.id;
    const navFilter = state.navFilter;
    searchDebounceTimer = setTimeout(async () => {
      try {
        const filter = buildPromptFilter(userId, navFilter);
        const prompts = await api.searchPrompts(userId, query, filter);
        actions.setPrompts(prompts);
      } catch (err) {
        actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
      }
    }, 250);
  }

  async function handleNewPrompt(): Promise<void> {
    if (!state.activeUser) return;
    actions.setActiveTab("prompts");
    try {
      const prompt = await api.createPrompt({
        user_id: state.activeUser.id,
        title: "Untitled Prompt",
        content: DEFAULT_PROMPT_CONTENT,
        tag_ids: [],
        category_ids: [],
        collection_ids: [],
      });
      actions.setPrompts([prompt, ...state.prompts]);
      await selectPrompt(prompt.id);
      actions.addToast("success", "Prompt Created", `"${prompt.title}" created`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleCopy(): Promise<void> {
    if (!state.activePromptDetail) return;
    const content = state.activePromptDetail.prompt.content;
    const title = state.activePromptDetail.prompt.title;
    const vars = extractTemplateVariables(content);
    if (vars.length > 0) {
      actions.openTemplateDialog(vars, content, title);
      return;
    }
    const clipOk = writeToSystemClipboard(content);
    if (clipOk) {
      actions.addToast("success", "Copied", "Content copied to clipboard");
    } else {
      actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
    }
    api.copyToClipboard(content, title).catch(() => {});
  }

  async function handleDuplicate(): Promise<void> {
    if (!state.activePromptDetail) return;
    try {
      const copy = await api.duplicatePrompt(state.activePromptDetail.prompt.id);
      actions.setPrompts([copy, ...state.prompts]);
      await selectPrompt(copy.id);
      actions.addToast("success", "Duplicated", `"${copy.title}" created`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleNewScript(): Promise<void> {
    if (!state.activeUser) return;
    actions.setActiveTab("scripts");
    try {
      const script = await api.createScript({
        user_id: state.activeUser.id,
        title: "new_script.txt",
        content: "# New script",
        script_language: "text",
        tag_ids: [],
        category_ids: [],
        collection_ids: [],
      });
      actions.setScripts([script, ...state.scripts]);
      actions.addToast("success", "Script Created", `"${script.title}" created`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleNewChain(): Promise<void> {
    if (!state.activeUser) return;
    actions.setActiveTab("chains");

    if (state.prompts.length === 0 && state.scripts.length === 0) {
      actions.addToast("info", "No Steps", "Create at least one prompt or script before creating a chain.");
      return;
    }

    const firstStep = state.prompts.length > 0
      ? { step_type: "prompt" as const, item_id: state.prompts[0]!.id }
      : { step_type: "script" as const, item_id: state.scripts[0]!.id };

    try {
      const chain = await api.createChain({
        user_id: state.activeUser.id,
        title: "Untitled Chain",
        prompt_ids: [],
        steps: [firstStep],
        tag_ids: [],
        category_ids: [],
        collection_ids: [],
      });
      actions.setChains([chain, ...state.chains]);
      actions.addToast("success", "Chain Created", `"${chain.title}" created`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  /** Called when the WelcomeDialog completes: hides the dialog and boots the app. */
  async function handleWelcomeComplete(): Promise<void> {
    setShowWelcome(false);
    actions.setLoading(true);
    await initializeApp();
  }

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
  return (
    <ErrorBoundary fallback={(err) => (
      <main class="splash">
        <h1 class="splash-title">
          <span class="title-neuron">Neuron</span><span class="title-prompter">Prompter</span>
        </h1>
        <p class="splash-error">Something went wrong: {err?.message ?? String(err)}</p>
        <button class="btn btn-primary" style="margin-top: 16px" onClick={() => window.location.reload()}>
          Reload
        </button>
      </main>
    )}>
      <Show when={showWelcome()}>
        <WelcomeDialog onClose={() => void handleWelcomeComplete()} />
      </Show>

      <Show when={needsLogin()}>
        <LoginPage onLogin={async (user) => {
          setNeedsLogin(false);
          actions.setLoading(true);
          actions.setActiveUser(user);
          if (needsMarkerRepair()) {
            try {
              await api.setupComplete();
            } catch {
              // Marker repair is non-critical; retried on next launch.
            }
            setNeedsMarkerRepair(false);
          }
          await initializeApp();
        }} />
      </Show>

      <Show
        when={!state.loading}
        fallback={
          <main class="splash">
            <h1 class="splash-title">
              <span class="title-neuron">Neuron</span><span class="title-prompter">Prompter</span>
            </h1>
            <p class="splash-status">Loading...</p>
          </main>
        }
      >
        <Show
          when={!state.loadError}
          fallback={
            <main class="splash">
              <h1 class="splash-title">
                <span class="title-neuron">Neuron</span><span class="title-prompter">Prompter</span>
              </h1>
              <p class="splash-error">{state.loadError}</p>
            </main>
          }
        >
          <div class="app-layout">
            <Topbar
              activeTab={state.activeTab}
              onTabChange={(tab) => handleTabChange(tab)}
            />

            <div class="app-body" role="tabpanel" id={`tabpanel-${state.activeTab}`} tabindex={-1}>
              {/* L-56: Each tab is wrapped in an ErrorBoundary so a crash in one
                  tab does not tear down the entire application. The fallback
                  shows the error and a reload button. */}
              <Switch>
                <Match when={state.activeTab === "organize"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <OrganizeTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "prompts"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <PromptsTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "chains"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <ChainsTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "scripts"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <ScriptsTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "clipboard"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <ClipboardTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "users"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <UsersTab onUserSwitch={loadUserData} />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "models"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <ModelsTab />
                  </ErrorBoundary>
                </Match>
                <Match when={state.activeTab === "settings"}>
                  <ErrorBoundary fallback={(err) => <TabErrorFallback error={err} />}>
                    <SettingsTab />
                  </ErrorBoundary>
                </Match>
              </Switch>
            </div>

            <Show when={state.logPanelOpen}>
              <LogPanel />
            </Show>

            <StatusBar
              ollamaConnected={state.ollamaConnected}
              mcpRegistered={state.mcpClaudeCode || state.mcpClaudeDesktop}
              promptCount={state.prompts.length}
              scriptCount={state.scripts.length}
              chainCount={state.chains.length}
              logPanelOpen={state.logPanelOpen}
              logMessageCount={state.logMessages.length}
              onToggleLogPanel={() => actions.toggleLogPanel()}
            />
          </div>
        </Show>
      </Show>

      <Toast />

      {/* Template variable substitution dialog. Rendered at the root level so it
       *  overlays the entire application when a prompt with {{variables}} is copied.
       *  The dialog collects user-supplied values for each variable and writes the
       *  substituted content to the system clipboard. */}
      <TemplateDialog
        open={state.templateDialogOpen}
        variables={state.templateVariables}
        content={state.templateContent}
        promptTitle={state.templatePromptTitle}
        onClose={() => actions.setTemplateDialogOpen(false)}
      />

      <UnsavedChangesDialog
        open={state.unsavedDialogOpen}
        onSave={() => state.unsavedDialogCallbacks?.onSave()}
        onDiscard={() => state.unsavedDialogCallbacks?.onDiscard()}
        onCancel={() => state.unsavedDialogCallbacks?.onCancel()}
      />
    </ErrorBoundary>
  );
};

/**
 * Fallback UI rendered inside a per-tab ErrorBoundary when a tab component
 * throws during rendering. Displays the error message and a reload button.
 */
function TabErrorFallback(props: { error: Error }): JSX.Element {
  return (
    <div style={{ padding: "var(--space-lg)", color: "var(--color-error)", "text-align": "center" }}>
      <p>This tab encountered an error: {props.error?.message ?? String(props.error)}</p>
      <button class="btn btn-primary" style="margin-top: 12px" onClick={() => window.location.reload()}>
        Reload
      </button>
    </div>
  );
}

export default App;
