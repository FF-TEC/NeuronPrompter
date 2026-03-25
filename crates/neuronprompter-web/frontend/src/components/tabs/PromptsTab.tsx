import { logError } from "../../utils/errors";
import { Component, createSignal, onCleanup } from "solid-js";
import { state, actions, guardNavigation } from "../../stores/app";
import { api, ApiError, writeToSystemClipboard, extractTemplateVariables } from "../../api/client";
import type { NavFilter, PromptFilter } from "../../api/types";
import { DEFAULT_PROMPT_CONTENT } from "../../api/types";
import SplitPane from "../SplitPane";
import PromptEditor from "../PromptEditor";
import PromptList from "../PromptList";
import "./PromptsTab.css";

/**
 * PromptsTab: wrapper composing SplitPane with PromptEditor (left)
 * and PromptList (right). Orchestrates all prompt-related actions.
 */

const PromptsTab: Component = () => {
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let selectCounter = 0;

  onCleanup(() => { if (debounceTimer) clearTimeout(debounceTimer); });

  const [navFilter, setNavFilter] = createSignal<NavFilter>({ kind: "all" });

  /** Propagates editor dirty state to the global store for navigation guards. */
  const handleDirtyChange = (dirty: boolean) => actions.setEditorDirty(dirty);

  /** Build a PromptFilter from NavFilter for the API call. */
  const buildFilter = (filter: NavFilter): PromptFilter => {
    const base: PromptFilter = { user_id: state.activeUser?.id };
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

  /** Reload prompts with current filter. */
  const reloadPrompts = async () => {
    if (!state.activeUser) return;
    try {
      const prompts = (await api.listPrompts(buildFilter(navFilter()))).items;
      actions.setPrompts(prompts);
    } catch (e) {
      logError("PromptsTab.reloadPrompts", e);
      actions.addToast("error", "Reload Failed", e instanceof Error ? e.message : String(e));
    }
  };

  /** Reload prompt detail for the active prompt. */
  const reloadDetail = async () => {
    if (!state.activePromptId) return;
    try {
      const detail = await api.getPrompt(state.activePromptId);
      actions.setActivePromptDetail(detail);
    } catch (e) {
      logError("PromptsTab.reloadDetail", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleSelectPrompt = async (id: number) => {
    if (id === state.activePromptId) return;
    const result = await guardNavigation();
    if (result === "cancel") return;
    if (result === "save" && state.saveHandler) {
      try { await state.saveHandler(); } catch { return; }
    }
    actions.setActivePromptId(id);
    const myCounter = ++selectCounter;
    try {
      const detail = await api.getPrompt(id);
      if (selectCounter !== myCounter) return; // stale response
      actions.setActivePromptDetail(detail);
    } catch (e) {
      if (selectCounter !== myCounter) return;
      logError("PromptsTab.loadPrompt", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleFilterChange = async (filter: NavFilter) => {
    setNavFilter(filter);
    if (!state.activeUser) return;
    try {
      const prompts = (await api.listPrompts(buildFilter(filter))).items;
      actions.setPrompts(prompts);
    } catch (e) {
      logError("PromptsTab.filterPrompts", e);
      actions.addToast("error", "Filter Failed", e instanceof Error ? e.message : String(e));
    }
  };

  const handleSearchChange = async (query: string) => {
    actions.setSearchQuery(query);
    if (!state.activeUser) return;
    if (!query.trim()) {
      if (debounceTimer) clearTimeout(debounceTimer);
      await reloadPrompts();
      return;
    }
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      try {
        const prompts = await api.searchPrompts(state.activeUser!.id, query);
        actions.setPrompts(prompts);
      } catch {
        // Keep current list on error
      }
    }, 300);
  };

  const handleSave = async () => {
    await reloadPrompts();
    await reloadDetail();
  };

  const handleCopy = async () => {
    if (!state.activePromptDetail) return;
    const p = state.activePromptDetail.prompt;
    // Check for template variables client-side (no network needed).
    const vars = extractTemplateVariables(p.content);
    if (vars.length > 0) {
      actions.openTemplateDialog(vars, p.content, p.title);
      return;
    }
    // Write to clipboard IMMEDIATELY (must happen within user-gesture context).
    const clipOk = writeToSystemClipboard(p.content);
    if (clipOk) {
      actions.addToast("success", "Copied", `"${p.title}" copied to clipboard`);
    } else {
      actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
    }
    // Record in backend history (fire-and-forget).
    api.copyToClipboard(p.content, p.title).catch(() => {});
  };

  const handleDelete = async () => {
    if (!state.activePromptDetail) return;
    if (!window.confirm("Delete this prompt?")) return;
    const id = state.activePromptDetail.prompt.id;
    try {
      await api.deletePrompt(id);
      actions.setActivePromptId(null);
      actions.setActivePromptDetail(null);
      await reloadPrompts();
    } catch (e) {
      // Handle 409 Conflict: prompt is used in chains.
      if (e instanceof ApiError && e.status === 409) {
        try {
          const body = JSON.parse(e.body);
          if (body.code === "ENTITY_IN_USE") {
            actions.addToast(
              "error",
              "Cannot Delete",
              body.message || "This prompt is used in one or more chains. Remove it from all chains first.",
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
    if (!state.activePromptDetail) return;
    try {
      const newPrompt = await api.duplicatePrompt(state.activePromptDetail.prompt.id);
      await reloadPrompts();
      await handleSelectPrompt(newPrompt.id);
    } catch (e) {
      logError("PromptsTab.duplicate", e);
      actions.addToast("error", "Duplicate Failed", e instanceof Error ? e.message : String(e));
    }
  };

  const handleArchiveToggle = async (_promptId: number, _isArchived: boolean) => {
    await reloadPrompts();
    await reloadDetail();
  };

  const handleVersionRestore = async () => {
    await reloadPrompts();
    await reloadDetail();
  };

  const handleToggleFavorite = async (promptId: number, isFavorite: boolean) => {
    try {
      await api.toggleFavorite(promptId, isFavorite);
      const prompts = state.prompts.map((p) =>
        p.id === promptId ? { ...p, is_favorite: isFavorite } : p,
      );
      actions.setPrompts(prompts);
      if (state.activePromptDetail && state.activePromptDetail.prompt.id === promptId) {
        await reloadDetail();
      }
    } catch (e) {
      logError("PromptsTab.toggleFavorite", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  const handleNewPrompt = async () => {
    if (!state.activeUser) return;
    try {
      const prompt = await api.createPrompt({
        user_id: state.activeUser.id,
        title: "Untitled Prompt",
        content: DEFAULT_PROMPT_CONTENT,
        tag_ids: [],
        category_ids: [],
        collection_ids: [],
      });
      await reloadPrompts();
      await handleSelectPrompt(prompt.id);
    } catch (e) {
      logError("PromptsTab.createPrompt", e);
      actions.addToast("error", "Create Failed", e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div class="prompts-tab">
      <SplitPane
        storageKey="prompts"
        defaultRatio={0.55}
        minLeftPx={400}
        minRightPx={280}
        left={
          <PromptEditor
            detail={state.activePromptDetail}
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
            ollamaBaseUrl={state.ollamaUrl}
            ollamaModel={state.ollamaModel}
            ollamaConnected={state.ollamaConnected}
          />
        }
        right={
          <PromptList
            prompts={state.prompts}
            activePromptId={state.activePromptId}
            tags={state.tags}
            categories={state.categories}
            collections={state.collections}
            activeFilter={navFilter()}
            searchQuery={state.searchQuery}
            onSelect={handleSelectPrompt}
            onToggleFavorite={handleToggleFavorite}
            onFilterChange={handleFilterChange}
            onSearchChange={handleSearchChange}
            onNewPrompt={handleNewPrompt}
          />
        }
      />
    </div>
  );
};

export default PromptsTab;
