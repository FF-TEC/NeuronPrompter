import { Component, For, Show, createSignal, createEffect } from "solid-js";
import { state, actions, applyTheme } from "../../stores/app";
import { api } from "../../api/client";
import { seedExamples, hasExamplesBeenSeeded, removeExamples } from "../../api/examples";
import { showSaveDialog, showOpenFileDialog, showOpenDirDialog } from "../../api/dialogs";
import type { ClipboardEntry, ImportSummary } from "../../api/types";
import McpPanel from "../panels/McpPanel";
import "./SettingsTab.css";

/**
 * SettingsTab: full settings interface as stacked glass cards.
 *
 * Contains Appearance, Import/Export (with native file dialogs),
 * MCP Server, and Clipboard History sections.
 */

const SettingsTab: Component = () => {
  // -------------------------------------------------------------------------
  // Theme state
  // -------------------------------------------------------------------------
  const [currentTheme, setCurrentTheme] = createSignal<"light" | "dark" | "system">("dark");

  // -------------------------------------------------------------------------
  // IO state
  // -------------------------------------------------------------------------
  const [exporting, setExporting] = createSignal(false);
  const [importing, setImporting] = createSignal(false);
  const [backingUp, setBackingUp] = createSignal(false);
  const [lastImportResult, setLastImportResult] = createSignal<ImportSummary | null>(null);

  // -------------------------------------------------------------------------
  // Example content state
  // -------------------------------------------------------------------------
  const [examplesSeeded, setExamplesSeeded] = createSignal(false);
  const [seedingExamples, setSeedingExamples] = createSignal(false);

  // -------------------------------------------------------------------------
  // Clipboard history state
  // -------------------------------------------------------------------------
  const [clipboardHistory, setClipboardHistory] = createSignal<ClipboardEntry[]>([]);
  const [showClipboardHistory, setShowClipboardHistory] = createSignal(false);

  // -------------------------------------------------------------------------
  // Load settings on mount
  // -------------------------------------------------------------------------
  createEffect(() => {
    if (state.activeUser) {
      loadSettings();
    }
  });

  /**
   * Fetches the user's settings from the backend and caches them in both the
   * local currentTheme signal (for the theme toggle UI) and the global store
   * (via actions.setUserSettings) so that subsequent operations like setTheme
   * can read the cached settings without making another API call.
   */
  async function loadSettings(): Promise<void> {
    if (!state.activeUser) return;
    try {
      const settings = await api.getUserSettings(state.activeUser.id);
      actions.setUserSettings(settings);
      setCurrentTheme(settings.theme);
    } catch (err) {
      actions.addToast("error", "Settings Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Example content check
  // -------------------------------------------------------------------------
  createEffect(() => {
    if (state.activeUser) {
      hasExamplesBeenSeeded(state.activeUser.id)
        .then(setExamplesSeeded)
        .catch(() => {});
    }
  });

  /**
   * Seeds example content for the active user and updates the seeded flag.
   */
  async function handleSeedExamples(): Promise<void> {
    if (!state.activeUser) return;
    const userId = state.activeUser.id;
    setSeedingExamples(true);
    try {
      await seedExamples(userId);
      setExamplesSeeded(true);

      // Refresh cached settings so the updated extra field (with
      // examples_seeded flag and entity IDs) is not overwritten by
      // subsequent settings writes (e.g. theme changes).
      const [settings, promptPage, scriptPage, chainPage] = await Promise.all([
        api.getUserSettings(userId),
        api.listPrompts({ user_id: userId }),
        api.listScripts({ user_id: userId, is_archived: false }),
        api.listChains({ user_id: userId }),
      ]);
      actions.setUserSettings(settings);
      actions.setPrompts(promptPage.items);
      actions.setScripts(scriptPage.items);
      actions.setChains(chainPage.items);

      actions.addToast("success", "Examples Created",
        "2 prompts, 1 script, and 1 chain have been added to your library.");
    } catch (err) {
      actions.addToast("error", "Error",
        err instanceof Error ? err.message : String(err));
    } finally {
      setSeedingExamples(false);
    }
  }

  /**
   * Deletes the seeded example entities and clears the seeded flag.
   * Refreshes the store lists and cached settings so removed entities
   * disappear without a reload. When some entities cannot be deleted
   * (e.g. referenced by a user-created chain), the flag stays active
   * and an error toast is shown.
   */
  async function handleRemoveExamples(): Promise<void> {
    if (!state.activeUser) return;
    const userId = state.activeUser.id;
    try {
      await removeExamples(userId);
      setExamplesSeeded(false);

      // Refresh cached settings so the cleared extra field is not
      // overwritten by subsequent settings writes (e.g. theme changes).
      const [settings, promptPage, scriptPage, chainPage] = await Promise.all([
        api.getUserSettings(userId),
        api.listPrompts({ user_id: userId }),
        api.listScripts({ user_id: userId, is_archived: false }),
        api.listChains({ user_id: userId }),
      ]);
      actions.setUserSettings(settings);
      actions.setPrompts(promptPage.items);
      actions.setScripts(scriptPage.items);
      actions.setChains(chainPage.items);

      actions.addToast("info", "Removed", "Example content has been deleted.");
    } catch (err) {
      // Partial failure: some entities were deleted but others survived
      // (e.g. a prompt referenced by a user-created chain). Refresh
      // lists to reflect whatever was deleted, and refresh settings
      // to keep the store cache in sync with the updated surviving IDs.
      const [settings, promptPage, scriptPage, chainPage] = await Promise.all([
        api.getUserSettings(userId),
        api.listPrompts({ user_id: userId }),
        api.listScripts({ user_id: userId, is_archived: false }),
        api.listChains({ user_id: userId }),
      ]);
      actions.setUserSettings(settings);
      actions.setPrompts(promptPage.items);
      actions.setScripts(scriptPage.items);
      actions.setChains(chainPage.items);

      // Re-check the seeded flag from the refreshed settings.
      setExamplesSeeded(await hasExamplesBeenSeeded(userId));

      actions.addToast("error", "Partial Removal",
        err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Theme management
  // -------------------------------------------------------------------------

  /**
   * L-57: Updates the theme preference both locally and on the server.
   * Reads the current settings from the global store (state.userSettings),
   * which was populated by loadSettings on mount. If the store does not have
   * settings yet (e.g. loadSettings has not completed), falls back to a
   * single API call. This eliminates the previous pattern where every theme
   * change triggered a redundant getUserSettings call even though the
   * settings had already been fetched during mount.
   */
  async function setTheme(value: "light" | "dark" | "system"): Promise<void> {
    setCurrentTheme(value);
    applyTheme(value);
    if (!state.activeUser) return;
    try {
      // Use the already-loaded settings from the store when available.
      // Only fetch from the API if the store has not been populated yet.
      const baseSettings = state.userSettings
        ?? await api.getUserSettings(state.activeUser.id);
      const updated = { ...baseSettings, theme: value };
      await api.updateUserSettings(updated);
      actions.setUserSettings(updated);
    } catch (err) {
      actions.addToast("error", "Theme Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Export handlers
  // -------------------------------------------------------------------------
  async function handleExportJson(): Promise<void> {
    if (!state.activeUser) return;
    const path = await showSaveDialog(
      "Export Prompts as JSON",
      [{ name: "JSON", extensions: ["json"] }],
      "NeuronPrompter-export.json",
    );
    if (!path) return;

    setExporting(true);
    try {
      const allPrompts = (await api.listPrompts({ user_id: state.activeUser.id })).items;
      const ids = allPrompts.map((p) => p.id);
      await api.exportJson(state.activeUser.id, ids, path);
      actions.addToast("success", "Exported", `${ids.length} prompts exported to ${path}`);
    } catch (err) {
      actions.addToast("error", "Export Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setExporting(false);
    }
  }

  async function handleExportMarkdown(): Promise<void> {
    if (!state.activeUser) return;
    const dirPath = await showOpenDirDialog();
    if (!dirPath) return;

    setExporting(true);
    try {
      const allPrompts = (await api.listPrompts({ user_id: state.activeUser.id })).items;
      const ids = allPrompts.map((p) => p.id);
      await api.exportMarkdown(state.activeUser.id, ids, dirPath);
      actions.addToast("success", "Exported", `${ids.length} prompts exported as Markdown to ${dirPath}`);
    } catch (err) {
      actions.addToast("error", "Export Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setExporting(false);
    }
  }

  async function handleBackup(): Promise<void> {
    const path = await showSaveDialog(
      "Backup Database",
      [{ name: "SQLite Database", extensions: ["db", "sqlite"] }],
      "NeuronPrompter-backup.db",
    );
    if (!path) return;

    setBackingUp(true);
    try {
      await api.backupDatabase(path);
      actions.addToast("success", "Backup Created", `Database backup saved to ${path}`);
    } catch (err) {
      actions.addToast("error", "Backup Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setBackingUp(false);
    }
  }

  // -------------------------------------------------------------------------
  // Import handlers
  // -------------------------------------------------------------------------
  async function handleImportJson(): Promise<void> {
    if (!state.activeUser) return;
    const path = await showOpenFileDialog(
      "Import JSON",
      [{ name: "JSON", extensions: ["json"] }],
    );
    if (!path) return;

    setImporting(true);
    setLastImportResult(null);
    try {
      const summary = await api.importJson(state.activeUser.id, path);
      setLastImportResult(summary);
      actions.addToast("success", "Imported",
        `${summary.prompts_imported} prompts imported`);
    } catch (err) {
      actions.addToast("error", "Import Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setImporting(false);
    }
  }

  async function handleImportMarkdown(): Promise<void> {
    if (!state.activeUser) return;
    const dirPath = await showOpenDirDialog();
    if (!dirPath) return;

    setImporting(true);
    setLastImportResult(null);
    try {
      const summary = await api.importMarkdown(state.activeUser.id, dirPath);
      setLastImportResult(summary);
      actions.addToast("success", "Imported",
        `${summary.prompts_imported} prompts imported from Markdown`);
    } catch (err) {
      actions.addToast("error", "Import Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setImporting(false);
    }
  }

  // -------------------------------------------------------------------------
  // Clipboard history
  // -------------------------------------------------------------------------
  async function loadClipboardHistory(): Promise<void> {
    try {
      const history = await api.getClipboardHistory();
      setClipboardHistory(history);
      setShowClipboardHistory(true);
    } catch (err) {
      actions.addToast("error", "History Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleClearClipboardHistory(): Promise<void> {
    try {
      await api.clearClipboardHistory();
      setClipboardHistory([]);
      actions.addToast("info", "Cleared", "Clipboard history cleared");
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Helpers
  // -------------------------------------------------------------------------
  function formatTimestamp(iso: string): string {
    return new Date(iso).toLocaleString("en-US", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function themeIcon(option: string) {
    if (option === "dark") {
      return <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M13.5 8.5a5.5 5.5 0 01-6-6 5.5 5.5 0 106 6z" stroke="currentColor" stroke-width="1.2"/></svg>;
    } else if (option === "light") {
      return <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="3" stroke="currentColor" stroke-width="1.2"/><path d="M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M4.1 4.1l1.1 1.1M10.8 10.8l1.1 1.1M4.1 11.9l1.1-1.1M10.8 5.2l1.1-1.1" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>;
    } else {
      return <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect x="2" y="3" width="12" height="10" rx="1.5" stroke="currentColor" stroke-width="1.2"/><path d="M2 6h12" stroke="currentColor" stroke-width="1.2"/></svg>;
    }
  }

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
  return (
    <div class="settings-tab">
      <div class="settings-container">

        {/* Appearance */}
        <div class="glass-card">
          <h3 class="card-title">Appearance</h3>
          <p class="card-description">Per-user color theme preference.</p>
          <div class="theme-options">
            <For each={["dark", "light", "system"] as const}>
              {(themeOption) => (
                <button
                  class="theme-btn"
                  classList={{ active: currentTheme() === themeOption }}
                  onClick={() => setTheme(themeOption)}
                  attr:data-tooltip={themeOption === "dark" ? "Use dark color theme" : themeOption === "light" ? "Use light color theme" : "Follow your operating system's theme preference"}
                >
                  {themeIcon(themeOption)}
                  <span>{themeOption.charAt(0).toUpperCase() + themeOption.slice(1)}</span>
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Export */}
        <div class="glass-card">
          <h3 class="card-title">Export</h3>
          <p class="card-description">Export prompts of the active user. Database backup includes all users.</p>
          <div class="io-actions">
            <button
              class="btn-io"
              onClick={handleExportJson}
              disabled={exporting()}
              data-tooltip="Export all prompts with versions and taxonomy as JSON"
            >
              {exporting() ? "Exporting..." : "Export JSON"}
            </button>
            <button
              class="btn-io"
              onClick={handleExportMarkdown}
              disabled={exporting()}
              data-tooltip="Export all prompts as Markdown files with YAML front-matter"
            >
              {exporting() ? "Exporting..." : "Export Markdown"}
            </button>
            <button
              class="btn-io"
              onClick={handleBackup}
              disabled={backingUp()}
              data-tooltip="Create a full copy of the SQLite database"
            >
              {backingUp() ? "Backing up..." : "Backup Database"}
            </button>
          </div>
        </div>

        {/* Import */}
        <div class="glass-card">
          <h3 class="card-title">Import</h3>
          <p class="card-description">Import prompts into the active user's library. Missing tags and categories are created automatically.</p>
          <div class="io-actions">
            <button
              class="btn-io"
              onClick={handleImportJson}
              disabled={importing()}
              data-tooltip="Import prompts from a NeuronPrompter JSON export file"
            >
              {importing() ? "Importing..." : "Import JSON"}
            </button>
            <button
              class="btn-io"
              onClick={handleImportMarkdown}
              disabled={importing()}
              data-tooltip="Import prompts from a directory of Markdown files with YAML front-matter"
            >
              {importing() ? "Importing..." : "Import Markdown"}
            </button>
          </div>

          {/* Import result summary */}
          <Show when={lastImportResult()}>
            {(result) => (
              <div class="import-result">
                <h4 class="result-title">Import Complete</h4>
                <Show when={result().source_user}>
                  <p class="result-source">
                    From: {result().source_user!.display_name} (@{result().source_user!.username})
                  </p>
                </Show>
                <div class="result-stats">
                  <span class="result-stat">{result().prompts_imported} prompts imported</span>
                  <Show when={result().tags_created > 0}>
                    <span class="result-stat">{result().tags_created} tags created</span>
                  </Show>
                  <Show when={result().categories_created > 0}>
                    <span class="result-stat">{result().categories_created} categories created</span>
                  </Show>
                  <Show when={result().collections_created > 0}>
                    <span class="result-stat">{result().collections_created} collections created</span>
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </div>

        {/* Example Content */}
        <div class="glass-card">
          <h3 class="card-title">Example Content</h3>
          <p class="card-description">
            Add example prompts, scripts, and chains that demonstrate template
            variables and the chain workflow. Examples are standard entities —
            edit or delete them from the Prompts, Scripts, and Chains tabs.
          </p>
          <div class="io-actions">
            <Show
              when={!examplesSeeded()}
              fallback={
                <button
                  class="btn-io"
                  onClick={handleRemoveExamples}
                  data-tooltip="Delete the example prompts, scripts, and chains"
                >
                  Remove example content
                </button>
              }
            >
              <button
                class="btn-io"
                onClick={() => void handleSeedExamples()}
                disabled={seedingExamples()}
                data-tooltip="Create 2 example prompts, 1 script, and 1 chain"
              >
                {seedingExamples() ? "Creating..." : "Add example content"}
              </button>
            </Show>
          </div>
        </div>

        {/* MCP */}
        <div class="glass-card">
          <h3 class="card-title">MCP Server</h3>
          <McpPanel />
          <p class="mcp-hint">
            The MCP server exposes prompts to external AI tools via the
            Model Context Protocol on stdio transport.
          </p>
        </div>

        {/* Session */}
        <div class="glass-card">
          <h3 class="card-title">Session</h3>
          <p class="card-description">End your current session and return to the login screen.</p>
          <div class="io-actions">
            <button class="btn btn-danger" onClick={async () => {
              await api.logout();
              window.location.reload();
            }}>
              Logout
            </button>
          </div>
        </div>

        {/* Clipboard History */}
        <div class="glass-card">
          <h3 class="card-title">Clipboard History</h3>
          <Show
            when={showClipboardHistory()}
            fallback={
              <button class="btn-io" onClick={loadClipboardHistory} data-tooltip="Load clipboard copy history">Load History</button>
            }
          >
            <div class="clipboard-actions">
              <button class="btn-sm" onClick={loadClipboardHistory} data-tooltip="Refresh clipboard history">Refresh</button>
              <button
                class="btn-sm btn-danger-sm"
                onClick={handleClearClipboardHistory}
                disabled={clipboardHistory().length === 0}
                data-tooltip="Delete all clipboard history entries"
              >
                Clear All
              </button>
            </div>
            <Show when={clipboardHistory().length === 0}>
              <p class="empty-text">No clipboard history entries.</p>
            </Show>
            <Show when={clipboardHistory().length > 0}>
              <div class="clipboard-list">
                <For each={clipboardHistory()}>
                  {(entry) => (
                    <div class="clipboard-item">
                      <div class="clipboard-meta">
                        <span class="clipboard-title">{entry.prompt_title}</span>
                        <span class="clipboard-time">{formatTimestamp(entry.copied_at)}</span>
                      </div>
                      <p class="clipboard-content">
                        {entry.content.slice(0, 200)}{entry.content.length > 200 ? "..." : ""}
                      </p>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </div>

        {/* About: application version read from the health endpoint. Placed
         *  last because version information is reference-only and accessed
         *  less frequently than the operational sections above. */}
        <div class="glass-card">
          <h3 class="card-title">About</h3>
          <div class="about-info">
            <div class="about-row">
              <span class="about-label">Version</span>
              <span class="about-value">
                {state.appVersion ?? "..."}
              </span>
            </div>
            <div class="about-row">
              <span class="about-label">Database</span>
              <span class="about-value about-path">
                {state.dbPath ?? "..."}
              </span>
            </div>
            <div class="about-row">
              <span class="about-label">License</span>
              <span>MIT OR Apache-2.0</span>
            </div>
          </div>
        </div>

      </div>
    </div>
  );
};

export default SettingsTab;
