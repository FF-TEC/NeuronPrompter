import { logError } from "../../utils/errors";
import { Component, createSignal, onCleanup } from "solid-js";
import { state, actions, guardNavigation } from "../../stores/app";
import { api, ApiError, writeToSystemClipboard, extractTemplateVariables } from "../../api/client";
import type { NavFilter, ScriptFilter } from "../../api/types";
import SplitPane from "../SplitPane";
import ScriptEditor from "../ScriptEditor";
import ScriptList from "../ScriptList";
import "./ScriptsTab.css";

/**
 * ScriptsTab: wrapper composing SplitPane with ScriptEditor (left)
 * and ScriptList (right). Orchestrates all script-related actions.
 */

const ScriptsTab: Component = () => {
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let selectCounter = 0;

  onCleanup(() => { if (debounceTimer) clearTimeout(debounceTimer); });

  const [navFilter, setNavFilter] = createSignal<NavFilter>({ kind: "all" });

  /** Propagates editor dirty state to the global store for navigation guards. */
  const handleDirtyChange = (dirty: boolean) => actions.setScriptEditorDirty(dirty);

  /** Build a ScriptFilter from NavFilter for the API call. */
  const buildFilter = (filter: NavFilter): ScriptFilter => {
    const base: ScriptFilter = { user_id: state.activeUser?.id };
    switch (filter.kind) {
      case "all":
        return base;
      case "favorites":
        return { ...base, is_favorite: true };
      case "archive":
        return { ...base, is_archived: true };
      case "tag":
        return { ...base, tag_id: filter.id };
      case "category":
        return { ...base, category_id: filter.id };
      case "collection":
        return { ...base, collection_id: filter.id };
    }
  };

  /** Reload scripts with current filter. */
  const reloadScripts = async () => {
    if (!state.activeUser) return;
    try {
      const scripts = (await api.listScripts(buildFilter(navFilter()))).items;
      actions.setScripts(scripts);
    } catch (e) {
      logError("ScriptsTab.reloadScripts", e);
      actions.addToast("error", "Reload Failed", e instanceof Error ? e.message : String(e));
    }
  };

  /** Reload script detail for the active script. */
  const reloadDetail = async () => {
    if (!state.activeScriptId) return;
    try {
      const detail = await api.getScript(state.activeScriptId);
      actions.setActiveScriptDetail(detail);
    } catch (e) {
      logError("ScriptsTab.reloadDetail", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleSelectScript = async (id: number) => {
    if (id === state.activeScriptId) return;
    const result = await guardNavigation();
    if (result === "cancel") return;
    if (result === "save" && state.saveHandler) {
      try { await state.saveHandler(); } catch { return; }
    }
    actions.setActiveScriptId(id);
    const myCounter = ++selectCounter;
    try {
      const detail = await api.getScript(id);
      if (selectCounter !== myCounter) return; // stale response
      actions.setActiveScriptDetail(detail);
    } catch (e) {
      if (selectCounter !== myCounter) return;
      logError("ScriptsTab.loadScript", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleFilterChange = async (filter: NavFilter) => {
    setNavFilter(filter);
    if (!state.activeUser) return;
    try {
      const scripts = (await api.listScripts(buildFilter(filter))).items;
      actions.setScripts(scripts);
    } catch (e) {
      logError("ScriptsTab.filterScripts", e);
      actions.addToast("error", "Filter Failed", e instanceof Error ? e.message : String(e));
    }
  };

  const handleSearchChange = async (query: string) => {
    actions.setSearchQuery(query);
    if (!state.activeUser) return;
    if (!query.trim()) {
      if (debounceTimer) clearTimeout(debounceTimer);
      await reloadScripts();
      return;
    }
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      try {
        const scripts = await api.searchScripts(state.activeUser!.id, query);
        actions.setScripts(scripts);
      } catch {
        // Keep current list on error
      }
    }, 300);
  };

  const handleSave = async () => {
    await reloadScripts();
    await reloadDetail();
  };

  const handleCopy = async () => {
    if (!state.activeScriptDetail) return;
    const s = state.activeScriptDetail.script;
    const vars = extractTemplateVariables(s.content);
    if (vars.length > 0) {
      actions.openTemplateDialog(vars, s.content, s.title);
      return;
    }
    const clipOk = writeToSystemClipboard(s.content);
    if (clipOk) {
      actions.addToast("success", "Copied", `"${s.title}" copied to clipboard`);
    } else {
      actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
    }
    api.copyToClipboard(s.content, s.title).catch(() => {});
  };

  const handleDelete = async () => {
    if (!state.activeScriptDetail) return;
    if (!window.confirm("Delete this script?")) return;
    const id = state.activeScriptDetail.script.id;
    try {
      await api.deleteScript(id);
      actions.setActiveScriptId(null);
      actions.setActiveScriptDetail(null);
      await reloadScripts();
    } catch (e) {
      // Handle 409 Conflict: script is used in chains.
      if (e instanceof ApiError && e.status === 409) {
        try {
          const body = JSON.parse(e.body);
          if (body.code === "ENTITY_IN_USE") {
            actions.addToast(
              "error",
              "Cannot Delete",
              body.message || "This script is used in one or more chains. Remove it from all chains first.",
            );
            return;
          }
        } catch {
          // If parsing fails, show the raw message.
        }
      }
      actions.addToast("error", "Delete Failed", e instanceof Error ? e.message : String(e));
    }
  };

  const handleDuplicate = async () => {
    if (!state.activeScriptDetail) return;
    try {
      const newScript = await api.duplicateScript(state.activeScriptDetail.script.id);
      await reloadScripts();
      await handleSelectScript(newScript.id);
    } catch (e) {
      logError("ScriptsTab.duplicate", e);
      actions.addToast("error", "Duplicate Failed", e instanceof Error ? e.message : String(e));
    }
  };

  const handleArchiveToggle = async (_scriptId: number, _isArchived: boolean) => {
    await reloadScripts();
    await reloadDetail();
  };

  const handleVersionRestore = async () => {
    await reloadScripts();
    await reloadDetail();
  };

  const handleToggleFavorite = async (scriptId: number, isFavorite: boolean) => {
    try {
      await api.toggleScriptFavorite(scriptId, isFavorite);
      const scripts = state.scripts.map((s) =>
        s.id === scriptId ? { ...s, is_favorite: isFavorite } : s,
      );
      actions.setScripts(scripts);
      if (state.activeScriptDetail && state.activeScriptDetail.script.id === scriptId) {
        await reloadDetail();
      }
    } catch (e) {
      logError("ScriptsTab.toggleFavorite", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleNewScript = async () => {
    if (!state.activeUser) return;
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
      await reloadScripts();
      await handleSelectScript(script.id);
    } catch (e) {
      logError("ScriptsTab.createScript", e);
      actions.addToast("error", "Create Failed", e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div class="scripts-tab">
      <SplitPane
        storageKey="scripts"
        defaultRatio={0.55}
        minLeftPx={400}
        minRightPx={280}
        left={
          <ScriptEditor
            detail={state.activeScriptDetail}
            allTags={state.tags}
            allCategories={state.categories}
            allCollections={state.collections}
            activeUser={state.activeUser}
            onSave={handleSave}
            onDirtyChange={handleDirtyChange}
            onCopy={handleCopy}
            onDelete={handleDelete}
            onDuplicate={handleDuplicate}
            onArchiveToggle={handleArchiveToggle}
            onVersionRestore={handleVersionRestore}
            onImported={async (scriptId: number) => {
              await reloadScripts();
              await handleSelectScript(scriptId);
            }}
          />
        }
        right={
          <ScriptList
            scripts={state.scripts}
            activeScriptId={state.activeScriptId}
            tags={state.tags}
            categories={state.categories}
            collections={state.collections}
            activeFilter={navFilter()}
            searchQuery={state.searchQuery}
            onSelect={handleSelectScript}
            onToggleFavorite={handleToggleFavorite}
            onFilterChange={handleFilterChange}
            onSearchChange={handleSearchChange}
            onNewScript={handleNewScript}
          />
        }
      />
    </div>
  );
};

export default ScriptsTab;
