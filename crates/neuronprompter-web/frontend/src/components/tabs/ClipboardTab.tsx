import { Component, For, Show, createSignal, createMemo, createEffect, on } from "solid-js";
import { state, actions } from "../../stores/app";
import { api, writeToSystemClipboard, extractTemplateVariables } from "../../api/client";
import type { Prompt, Chain, Script, Tag, Category, Collection, PromptFilter, ChainFilter, ScriptFilter } from "../../api/types";
import SplitPane from "../SplitPane";
import "./ClipboardTab.css";

/**
 * ClipboardTab: Advanced search-and-copy workspace with SplitPane layout.
 *
 * Left panel provides comprehensive search configuration with combinable filters:
 * fulltext search, title filter, language filter, multi-select taxonomy chips
 * (tags/categories/collections), favorite/archive toggles, and sort options.
 *
 * Right panel shows matching prompt results as glass cards with copy buttons
 * and a collapsible clipboard history.
 */

type EnrichedPrompt = Prompt & { tags?: Tag[]; categories?: Category[]; collections?: Collection[] };
type EnrichedChain = Chain & { tags?: Tag[]; categories?: Category[]; collections?: Collection[] };
type EnrichedScript = Script & { tags?: Tag[]; categories?: Category[]; collections?: Collection[] };

/** Unified search result combining prompts, chains, and scripts. */
type SearchResultItem =
  | { kind: "prompt"; item: EnrichedPrompt }
  | { kind: "chain"; item: EnrichedChain }
  | { kind: "script"; item: EnrichedScript };

const ClipboardTab: Component = () => {
  // -------------------------------------------------------------------------
  // Search state
  // -------------------------------------------------------------------------
  const [fulltext, setFulltext] = createSignal("");
  const [titleFilter, setTitleFilter] = createSignal("");
  const [languageFilter, setLanguageFilter] = createSignal("");
  const [selectedTagIds, setSelectedTagIds] = createSignal<Set<number>>(new Set());
  const [selectedCategoryIds, setSelectedCategoryIds] = createSignal<Set<number>>(new Set());
  const [selectedCollectionIds, setSelectedCollectionIds] = createSignal<Set<number>>(new Set());
  const [favoriteFilter, setFavoriteFilter] = createSignal<"all" | "only" | "exclude">("all");
  const [includeArchived, setIncludeArchived] = createSignal(false);
  const [sortBy, setSortBy] = createSignal<"updated_at" | "created_at" | "title">("updated_at");
  const [sortDirection, setSortDirection] = createSignal<"asc" | "desc">("desc");
  const [typeFilter, setTypeFilter] = createSignal<"all" | "prompts" | "chains" | "scripts">("all");

  // -------------------------------------------------------------------------
  // Results state
  // -------------------------------------------------------------------------
  const [results, setResults] = createSignal<EnrichedPrompt[]>([]);
  const [chainResults, setChainResults] = createSignal<EnrichedChain[]>([]);
  const [scriptResults, setScriptResults] = createSignal<EnrichedScript[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [searched, setSearched] = createSignal(false);

  // -------------------------------------------------------------------------
  // Clipboard history state
  // -------------------------------------------------------------------------
  const [clipboardHistory, setClipboardHistory] = createSignal<{ content: string; prompt_title: string; copied_at: string }[]>([]);
  const [historyExpanded, setHistoryExpanded] = createSignal(false);

  // -------------------------------------------------------------------------
  // Languages
  // -------------------------------------------------------------------------
  const [availableLanguages, setAvailableLanguages] = createSignal<string[]>([]);

  // -------------------------------------------------------------------------
  // Debounce timer
  // -------------------------------------------------------------------------
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function debouncedSearch(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { performSearch(); }, 300);
  }

  // -------------------------------------------------------------------------
  // Active filter count
  // -------------------------------------------------------------------------
  const activeFilterCount = createMemo(() => {
    let count = 0;
    if (fulltext().trim()) count++;
    if (titleFilter().trim()) count++;
    if (languageFilter()) count++;
    if (selectedTagIds().size > 0) count++;
    if (selectedCategoryIds().size > 0) count++;
    if (selectedCollectionIds().size > 0) count++;
    if (favoriteFilter() !== "all") count++;
    if (includeArchived()) count++;
    if (typeFilter() !== "all") count++;
    return count;
  });

  // -------------------------------------------------------------------------
  // Load languages
  // -------------------------------------------------------------------------
  async function loadLanguages(): Promise<void> {
    if (!state.activeUser) return;
    try {
      // M-55: Use the dedicated languages endpoint instead of fetching all prompts.
      // This avoids transferring all prompt data just to extract language strings.
      const promptLangs = await api.listPromptLanguages();
      const scriptLangs = await api.listScriptLanguages();
      const combined = new Set<string>([...promptLangs, ...scriptLangs]);
      setAvailableLanguages([...combined].sort());
    } catch { /* ignore */ }
  }

  // -------------------------------------------------------------------------
  // Initial load
  // -------------------------------------------------------------------------
  // M-50: Track only the activeUser ID to avoid re-triggering the effect
  // on unrelated changes to the activeUser object (e.g. display_name updates).
  createEffect(on(() => state.activeUser?.id, (userId) => {
    if (userId !== null && userId !== undefined) {
      performSearch();
      loadLanguages();
    }
  }));

  // -------------------------------------------------------------------------
  // Search execution
  // -------------------------------------------------------------------------
  async function performSearch(): Promise<void> {
    if (!state.activeUser) return;
    setLoading(true);
    try {
      const backendFilter: PromptFilter = {
        user_id: state.activeUser.id,
        is_archived: includeArchived() ? undefined : false,
      };

      if (selectedTagIds().size === 1) {
        backendFilter.tag_id = [...selectedTagIds()][0];
      }
      if (selectedCategoryIds().size === 1 && !backendFilter.tag_id) {
        backendFilter.category_id = [...selectedCategoryIds()][0];
      }
      if (selectedCollectionIds().size === 1 && !backendFilter.tag_id && !backendFilter.category_id) {
        backendFilter.collection_id = [...selectedCollectionIds()][0];
      }

      if (favoriteFilter() === "only") {
        backendFilter.is_favorite = true;
      }

      // Fetch prompts, chains, and scripts in parallel.
      const chainFilter: ChainFilter = {
        user_id: state.activeUser.id,
        is_archived: includeArchived() ? undefined : false,
        is_favorite: favoriteFilter() === "only" ? true : undefined,
      };
      if (selectedTagIds().size === 1) {
        chainFilter.tag_id = [...selectedTagIds()][0];
      }
      if (selectedCategoryIds().size === 1 && !chainFilter.tag_id) {
        chainFilter.category_id = [...selectedCategoryIds()][0];
      }
      if (selectedCollectionIds().size === 1 && !chainFilter.tag_id && !chainFilter.category_id) {
        chainFilter.collection_id = [...selectedCollectionIds()][0];
      }

      const scriptFilter: ScriptFilter = {
        user_id: state.activeUser.id,
        is_archived: includeArchived() ? undefined : false,
        is_favorite: favoriteFilter() === "only" ? true : undefined,
      };
      if (selectedTagIds().size === 1) {
        scriptFilter.tag_id = [...selectedTagIds()][0];
      }
      if (selectedCategoryIds().size === 1 && !scriptFilter.tag_id) {
        scriptFilter.category_id = [...selectedCategoryIds()][0];
      }
      if (selectedCollectionIds().size === 1 && !scriptFilter.tag_id && !scriptFilter.category_id) {
        scriptFilter.collection_id = [...selectedCollectionIds()][0];
      }

      const tf = typeFilter();
      const fetchPrompts = tf === "all" || tf === "prompts";
      const fetchChains = tf === "all" || tf === "chains";
      const fetchScripts = tf === "all" || tf === "scripts";

      let rawResults: Prompt[];
      let rawChains: Chain[] = [];
      let rawScripts: Script[] = [];

      if (fulltext().trim()) {
        const [prompts, chains, scripts] = await Promise.all([
          fetchPrompts ? api.searchPrompts(state.activeUser.id, fulltext().trim(), backendFilter) : Promise.resolve([]),
          fetchChains ? api.searchChains(state.activeUser.id, fulltext().trim(), chainFilter) : Promise.resolve([]),
          fetchScripts ? api.searchScripts(state.activeUser.id, fulltext().trim(), scriptFilter) : Promise.resolve([]),
        ]);
        rawResults = prompts;
        rawChains = chains;
        rawScripts = scripts;
      } else {
        const [promptPage, chainPage, scriptPage] = await Promise.all([
          fetchPrompts ? api.listPrompts(backendFilter).then(r => r.items) : Promise.resolve([]),
          fetchChains ? api.listChains(chainFilter).then(r => r.items) : Promise.resolve([]),
          fetchScripts ? api.listScripts(scriptFilter).then(r => r.items) : Promise.resolve([]),
        ]);
        rawResults = promptPage;
        rawChains = chainPage;
        rawScripts = scriptPage;
      }

      const needsAssociations =
        selectedTagIds().size > 1 ||
        selectedCategoryIds().size > 1 ||
        selectedCollectionIds().size > 1 ||
        (selectedTagIds().size >= 1 && selectedCategoryIds().size >= 1) ||
        (selectedTagIds().size >= 1 && selectedCollectionIds().size >= 1) ||
        (selectedCategoryIds().size >= 1 && selectedCollectionIds().size >= 1);

      let enriched: EnrichedPrompt[];
      if (needsAssociations) {
        const details = await Promise.all(
          rawResults.map((p) => api.getPrompt(p.id).catch(() => null))
        );
        enriched = rawResults.map((p, i) => {
          const d = details[i];
          if (d) {
            return { ...p, tags: d.tags, categories: d.categories, collections: d.collections };
          }
          return p;
        });
      } else {
        enriched = rawResults;
      }

      setResults(applyClientFilters(enriched));

      // Enrich chains with association data when multi-taxonomy filtering is active.
      let enrichedChains: EnrichedChain[];
      if (needsAssociations) {
        const chainDetails = await Promise.all(
          rawChains.map((c) => api.getChain(c.id).catch(() => null))
        );
        enrichedChains = rawChains.map((c, i) => {
          const d = chainDetails[i];
          if (d) {
            return { ...c, tags: d.tags, categories: d.categories, collections: d.collections };
          }
          return c;
        });
      } else {
        enrichedChains = rawChains;
      }

      // Enrich scripts with association data when multi-taxonomy filtering is active.
      let enrichedScripts: EnrichedScript[];
      if (needsAssociations) {
        const scriptDetails = await Promise.all(
          rawScripts.map((s) => api.getScript(s.id).catch(() => null))
        );
        enrichedScripts = rawScripts.map((s, i) => {
          const d = scriptDetails[i];
          if (d) {
            return { ...s, tags: d.tags, categories: d.categories, collections: d.collections };
          }
          return s;
        });
      } else {
        enrichedScripts = rawScripts;
      }

      setChainResults(applyChainClientFilters(enrichedChains));
      setScriptResults(applyScriptClientFilters(enrichedScripts));
      setSearched(true);
    } catch (err) {
      actions.addToast("error", "Search Failed", err instanceof Error ? err.message : String(err));
      setResults([]);
      setScriptResults([]);
    } finally {
      setLoading(false);
    }
  }

  function applyClientFilters(items: EnrichedPrompt[]): EnrichedPrompt[] {
    return items.filter((p) => {
      if (titleFilter().trim() && !p.title.toLowerCase().includes(titleFilter().trim().toLowerCase())) {
        return false;
      }
      if (languageFilter() && p.language !== languageFilter()) {
        return false;
      }
      if (favoriteFilter() === "exclude" && p.is_favorite) {
        return false;
      }
      if (selectedTagIds().size > 0 && p.tags) {
        const pTagIds = new Set(p.tags.map((t) => t.id));
        for (const id of selectedTagIds()) {
          if (!pTagIds.has(id)) return false;
        }
      }
      if (selectedCategoryIds().size > 0 && p.categories) {
        const pCatIds = new Set(p.categories.map((c) => c.id));
        for (const id of selectedCategoryIds()) {
          if (!pCatIds.has(id)) return false;
        }
      }
      if (selectedCollectionIds().size > 0 && p.collections) {
        const pColIds = new Set(p.collections.map((c) => c.id));
        for (const id of selectedCollectionIds()) {
          if (!pColIds.has(id)) return false;
        }
      }
      return true;
    });
  }

  /**
   * Applies client-side filters to enriched chains.
   * Mirrors the taxonomy filtering logic used for prompts.
   */
  function applyChainClientFilters(items: EnrichedChain[]): EnrichedChain[] {
    return items.filter((c) => {
      if (titleFilter().trim() && !c.title.toLowerCase().includes(titleFilter().trim().toLowerCase())) {
        return false;
      }
      if (favoriteFilter() === "exclude" && c.is_favorite) {
        return false;
      }
      if (selectedTagIds().size > 0 && c.tags) {
        const ids = new Set(c.tags.map((t) => t.id));
        for (const id of selectedTagIds()) {
          if (!ids.has(id)) return false;
        }
      }
      if (selectedCategoryIds().size > 0 && c.categories) {
        const ids = new Set(c.categories.map((cat) => cat.id));
        for (const id of selectedCategoryIds()) {
          if (!ids.has(id)) return false;
        }
      }
      if (selectedCollectionIds().size > 0 && c.collections) {
        const ids = new Set(c.collections.map((col) => col.id));
        for (const id of selectedCollectionIds()) {
          if (!ids.has(id)) return false;
        }
      }
      return true;
    });
  }

  /**
   * Applies client-side filters to enriched scripts.
   * Mirrors the taxonomy filtering logic used for prompts.
   */
  function applyScriptClientFilters(items: EnrichedScript[]): EnrichedScript[] {
    return items.filter((s) => {
      if (titleFilter().trim() && !s.title.toLowerCase().includes(titleFilter().trim().toLowerCase())) {
        return false;
      }
      if (languageFilter() && s.language !== languageFilter()) {
        return false;
      }
      if (favoriteFilter() === "exclude" && s.is_favorite) {
        return false;
      }
      if (selectedTagIds().size > 0 && s.tags) {
        const ids = new Set(s.tags.map((t) => t.id));
        for (const id of selectedTagIds()) {
          if (!ids.has(id)) return false;
        }
      }
      if (selectedCategoryIds().size > 0 && s.categories) {
        const ids = new Set(s.categories.map((cat) => cat.id));
        for (const id of selectedCategoryIds()) {
          if (!ids.has(id)) return false;
        }
      }
      if (selectedCollectionIds().size > 0 && s.collections) {
        const ids = new Set(s.collections.map((col) => col.id));
        for (const id of selectedCollectionIds()) {
          if (!ids.has(id)) return false;
        }
      }
      return true;
    });
  }

  // -------------------------------------------------------------------------
  // Sorted results
  // -------------------------------------------------------------------------
  const sortedResults = createMemo(() => {
    const sorted = [...results()];
    sorted.sort((a, b) => {
      let cmp: number;
      const sb = sortBy();
      if (sb === "title") {
        cmp = a.title.localeCompare(b.title);
      } else {
        cmp = new Date(a[sb]).getTime() - new Date(b[sb]).getTime();
      }
      return sortDirection() === "desc" ? -cmp : cmp;
    });
    return sorted;
  });

  /** Unified results: prompts + chains + scripts merged and sorted. */
  const unifiedResults = createMemo<SearchResultItem[]>(() => {
    const items: SearchResultItem[] = [];
    for (const p of sortedResults()) {
      items.push({ kind: "prompt", item: p });
    }
    for (const c of chainResults()) {
      items.push({ kind: "chain", item: c });
    }
    for (const s of scriptResults()) {
      items.push({ kind: "script", item: s });
    }
    // Sort unified list by selected sort criteria.
    items.sort((a, b) => {
      const sb = sortBy();
      let cmp: number;
      if (sb === "title") {
        cmp = a.item.title.localeCompare(b.item.title);
      } else {
        const aTime = sb === "created_at" ? a.item.created_at : a.item.updated_at;
        const bTime = sb === "created_at" ? b.item.created_at : b.item.updated_at;
        cmp = new Date(aTime).getTime() - new Date(bTime).getTime();
      }
      return sortDirection() === "desc" ? -cmp : cmp;
    });
    return items;
  });

  const totalResultCount = createMemo(() => unifiedResults().length);

  // -------------------------------------------------------------------------
  // Copy handler
  // -------------------------------------------------------------------------
  async function handleCopy(prompt: Prompt): Promise<void> {
    const vars = extractTemplateVariables(prompt.content);
    if (vars.length > 0) {
      actions.openTemplateDialog(vars, prompt.content, prompt.title);
      return;
    }
    const clipOk = writeToSystemClipboard(prompt.content);
    if (clipOk) {
      actions.addToast("success", "Copied", `"${prompt.title}" copied to clipboard`);
    } else {
      actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
    }
    api.copyToClipboard(prompt.content, prompt.title).catch(() => {});
  }

  /** Copy a chain's composed content to clipboard. */
  async function handleCopyChain(chain: Chain): Promise<void> {
    try {
      const { content } = await api.getChainContent(chain.id);
      const vars = extractTemplateVariables(content);
      if (vars.length > 0) {
        actions.openTemplateDialog(vars, content, chain.title);
        return;
      }
      const clipOk = writeToSystemClipboard(content);
      if (clipOk) {
        actions.addToast("success", "Copied", `Chain "${chain.title}" copied to clipboard`);
      } else {
        actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
      }
      api.copyToClipboard(content, chain.title).catch(() => {});
    } catch (err) {
      actions.addToast("error", "Copy Failed", err instanceof Error ? err.message : String(err));
    }
  }

  /** Copy a script's content to clipboard. */
  async function handleCopyScript(script: Script): Promise<void> {
    const vars = extractTemplateVariables(script.content);
    if (vars.length > 0) {
      actions.openTemplateDialog(vars, script.content, script.title);
      return;
    }
    const clipOk = writeToSystemClipboard(script.content);
    if (clipOk) {
      actions.addToast("success", "Copied", `Script "${script.title}" copied to clipboard`);
    } else {
      actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
    }
    api.copyToClipboard(script.content, script.title).catch(() => {});
  }

  // -------------------------------------------------------------------------
  // Clipboard history
  // -------------------------------------------------------------------------
  async function loadHistory(): Promise<void> {
    try {
      const history = await api.getClipboardHistory();
      setClipboardHistory(history);
      setHistoryExpanded(true);
    } catch (err) {
      actions.addToast("error", "History Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Chip toggles
  // -------------------------------------------------------------------------
  function toggleTag(tagId: number): void {
    const next = new Set(selectedTagIds());
    if (next.has(tagId)) next.delete(tagId); else next.add(tagId);
    setSelectedTagIds(next);
    performSearch();
  }

  function toggleCategory(catId: number): void {
    const next = new Set(selectedCategoryIds());
    if (next.has(catId)) next.delete(catId); else next.add(catId);
    setSelectedCategoryIds(next);
    performSearch();
  }

  function toggleCollection(colId: number): void {
    const next = new Set(selectedCollectionIds());
    if (next.has(colId)) next.delete(colId); else next.add(colId);
    setSelectedCollectionIds(next);
    performSearch();
  }

  // -------------------------------------------------------------------------
  // Reset filters
  // -------------------------------------------------------------------------
  function resetFilters(): void {
    setFulltext("");
    setTitleFilter("");
    setLanguageFilter("");
    setSelectedTagIds(new Set<number>());
    setSelectedCategoryIds(new Set<number>());
    setSelectedCollectionIds(new Set<number>());
    setFavoriteFilter("all");
    setIncludeArchived(false);
    setTypeFilter("all");
    setSortBy("updated_at");
    setSortDirection("desc");
    performSearch();
  }

  // -------------------------------------------------------------------------
  // Helpers
  // -------------------------------------------------------------------------
  function previewContent(content: string): string {
    if (content.length <= 200) return content;
    return content.slice(0, 200) + "...";
  }

  function formatTimestamp(iso: string): string {
    return new Date(iso).toLocaleString("en-US", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function langLabel(code: string): string {
    const map: Record<string, string> = {
      en: "English", de: "Deutsch", fr: "Francais", es: "Espanol",
      it: "Italiano", pt: "Portugues", ja: "Japanese", zh: "Chinese",
      ko: "Korean", ru: "Russian", nl: "Dutch", pl: "Polish",
    };
    return map[code] ?? code.toUpperCase();
  }

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
  return (
    <div class="clipboard-tab">
      <SplitPane
        storageKey="clipboard"
        defaultRatio={0.30}
        minLeftPx={260}
        minRightPx={350}
        left={
          <div class="search-config">
            {/* Fulltext search */}
            <div class="search-section">
              <h3 class="section-label">Search</h3>
              <div class="input-with-icon">
                <svg class="input-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
                  <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.3"/>
                  <path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                </svg>
                <input
                  type="text"
                  class="search-input"
                  placeholder="Fulltext search..."
                  value={fulltext()}
                  onInput={(e) => { setFulltext(e.currentTarget.value); debouncedSearch(); }}
                  onKeyDown={(e) => { if (e.key === "Enter") performSearch(); }}
                  data-tooltip="Full-text search across all content"
                />
              </div>
            </div>

            {/* Field filters */}
            <div class="filter-group">
              <h4 class="group-label">Field Filters</h4>

              <div class="filter-section">
                <label class="filter-label" for="cb-title-filter">Title</label>
                <input
                  id="cb-title-filter"
                  type="text"
                  class="filter-input"
                  placeholder="Filter by title..."
                  value={titleFilter()}
                  onInput={(e) => { setTitleFilter(e.currentTarget.value); debouncedSearch(); }}
                  data-tooltip="Filter results by title substring"
                />
              </div>

              <div class="filter-section">
                <label class="filter-label" for="cb-lang-filter">Language</label>
                <select
                  id="cb-lang-filter"
                  class="filter-select"
                  value={languageFilter()}
                  onChange={(e) => { setLanguageFilter(e.currentTarget.value); performSearch(); }}
                  data-tooltip="Filter by content language"
                >
                  <option value="">All languages</option>
                  <For each={availableLanguages()}>
                    {(lang) => <option value={lang}>{langLabel(lang)}</option>}
                  </For>
                </select>
              </div>
            </div>

            {/* Taxonomy filters */}
            <div class="filter-group">
              <h4 class="group-label">Taxonomy</h4>

              <Show when={state.tags.length > 0}>
                <div class="filter-section">
                  <div class="filter-label-row">
                    <span class="filter-label">Tags</span>
                    <Show when={selectedTagIds().size > 0}>
                      <button class="clear-btn" onClick={() => { setSelectedTagIds(new Set<number>()); performSearch(); }} title="Clear tag filter">Clear</button>
                    </Show>
                  </div>
                  <div class="chip-container">
                    <For each={state.tags}>
                      {(tag) => (
                        <button
                          class="chip chip-tag"
                          classList={{ selected: selectedTagIds().has(tag.id) }}
                          onClick={() => toggleTag(tag.id)}
                        >
                          {tag.name}
                        </button>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              <Show when={state.categories.length > 0}>
                <div class="filter-section">
                  <div class="filter-label-row">
                    <span class="filter-label">Categories</span>
                    <Show when={selectedCategoryIds().size > 0}>
                      <button class="clear-btn" onClick={() => { setSelectedCategoryIds(new Set<number>()); performSearch(); }} title="Clear category filter">Clear</button>
                    </Show>
                  </div>
                  <div class="chip-container">
                    <For each={state.categories}>
                      {(cat) => (
                        <button
                          class="chip chip-category"
                          classList={{ selected: selectedCategoryIds().has(cat.id) }}
                          onClick={() => toggleCategory(cat.id)}
                        >
                          {cat.name}
                        </button>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              <Show when={state.collections.length > 0}>
                <div class="filter-section">
                  <div class="filter-label-row">
                    <span class="filter-label">Collections</span>
                    <Show when={selectedCollectionIds().size > 0}>
                      <button class="clear-btn" onClick={() => { setSelectedCollectionIds(new Set<number>()); performSearch(); }} title="Clear collection filter">Clear</button>
                    </Show>
                  </div>
                  <div class="chip-container">
                    <For each={state.collections}>
                      {(col) => (
                        <button
                          class="chip chip-collection"
                          classList={{ selected: selectedCollectionIds().has(col.id) }}
                          onClick={() => toggleCollection(col.id)}
                        >
                          {col.name}
                        </button>
                      )}
                    </For>
                  </div>
                </div>
              </Show>
            </div>

            {/* Status filters */}
            <div class="filter-group">
              <h4 class="group-label">Status</h4>

              <div class="filter-section">
                <span class="filter-label">Favorites</span>
                <div class="chip-container">
                  <For each={[
                    { value: "all" as const, label: "All", tooltip: "Show all items" },
                    { value: "only" as const, label: "Favorites", tooltip: "Show favorites only" },
                    { value: "exclude" as const, label: "No Favorites", tooltip: "Hide favorites" },
                  ]}>
                    {(opt) => (
                      <button
                        class="chip chip-filter"
                        classList={{ selected: favoriteFilter() === opt.value }}
                        onClick={() => { setFavoriteFilter(opt.value); performSearch(); }}
                        attr:data-tooltip={opt.tooltip}
                      >
                        {opt.label}
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <label class="toggle-row">
                <input
                  type="checkbox"
                  checked={includeArchived()}
                  onChange={(e) => { setIncludeArchived(e.currentTarget.checked); performSearch(); }}
                  data-tooltip="Include archived items in search results"
                />
                <span>Include archived</span>
              </label>
            </div>

            {/* Type filter */}
            <div class="filter-group">
              <h4 class="group-label">Type</h4>
              <div class="filter-section">
                <div class="chip-container">
                  <For each={[
                    { value: "all" as const, label: "All", tooltip: "Search all types" },
                    { value: "prompts" as const, label: "Prompts", tooltip: "Search prompts only" },
                    { value: "scripts" as const, label: "Scripts", tooltip: "Search scripts only" },
                    { value: "chains" as const, label: "Chains", tooltip: "Search chains only" },
                  ]}>
                    {(opt) => (
                      <button
                        class="chip chip-filter"
                        classList={{ selected: typeFilter() === opt.value }}
                        onClick={() => { setTypeFilter(opt.value); performSearch(); }}
                        attr:data-tooltip={opt.tooltip}
                      >
                        {opt.label}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </div>

            {/* Sort */}
            <div class="filter-group">
              <h4 class="group-label">Sort</h4>
              <div class="filter-section">
                <div class="chip-container">
                  <For each={[
                    { value: "updated_at" as const, label: "Modified", tooltip: "Sort by last modified date" },
                    { value: "created_at" as const, label: "Created", tooltip: "Sort by creation date" },
                    { value: "title" as const, label: "Title", tooltip: "Sort alphabetically by title" },
                  ]}>
                    {(opt) => (
                      <button
                        class="chip chip-filter"
                        classList={{ selected: sortBy() === opt.value }}
                        onClick={() => setSortBy(opt.value)}
                        attr:data-tooltip={opt.tooltip}
                      >
                        {opt.label}
                      </button>
                    )}
                  </For>
                  <button
                    class="chip chip-filter chip-dir"
                    onClick={() => setSortDirection(sortDirection() === "desc" ? "asc" : "desc")}
                    attr:data-tooltip={sortDirection() === "desc" ? "Currently descending — click to sort ascending" : "Currently ascending — click to sort descending"}
                  >
                    <svg
                      width="12" height="12" viewBox="0 0 16 16" fill="none"
                      class={sortDirection() === "asc" ? "flipped" : ""}
                    >
                      <path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                    {sortDirection() === "desc" ? "Desc" : "Asc"}
                  </button>
                </div>
              </div>
            </div>

            {/* Reset & filter count */}
            <div class="reset-section">
              <Show when={activeFilterCount() > 0}>
                <button class="btn-reset" onClick={resetFilters} title="Reset all search filters to defaults">
                  Reset all filters
                </button>
                <span class="filter-count-badge">
                  {activeFilterCount()} active filter{activeFilterCount() !== 1 ? "s" : ""}
                </span>
              </Show>
            </div>
          </div>
        }
        right={
          <div class="results-panel">
            <div class="results-header">
              <h3 class="results-count">
                <Show when={loading()} fallback={
                  <Show when={searched()} fallback="Search prompts to copy">
                    {totalResultCount()} result{totalResultCount() !== 1 ? "s" : ""}
                  </Show>
                }>
                  Searching...
                </Show>
              </h3>
              <Show when={activeFilterCount() > 0 && searched()}>
                <div class="active-filter-pills">
                  <Show when={fulltext().trim()}>
                    <span class="filter-pill">"{fulltext().trim()}"</span>
                  </Show>
                  <Show when={titleFilter().trim()}>
                    <span class="filter-pill">Title: {titleFilter().trim()}</span>
                  </Show>
                  <Show when={languageFilter()}>
                    <span class="filter-pill">{langLabel(languageFilter())}</span>
                  </Show>
                  <For each={[...selectedTagIds()]}>
                    {(tagId) => {
                      const tag = state.tags.find((t) => t.id === tagId);
                      return (
                        <Show when={tag}>
                          <span class="filter-pill pill-tag">{tag!.name}</span>
                        </Show>
                      );
                    }}
                  </For>
                  <For each={[...selectedCategoryIds()]}>
                    {(catId) => {
                      const cat = state.categories.find((c) => c.id === catId);
                      return (
                        <Show when={cat}>
                          <span class="filter-pill pill-category">{cat!.name}</span>
                        </Show>
                      );
                    }}
                  </For>
                  <For each={[...selectedCollectionIds()]}>
                    {(colId) => {
                      const col = state.collections.find((c) => c.id === colId);
                      return (
                        <Show when={col}>
                          <span class="filter-pill pill-collection">{col!.name}</span>
                        </Show>
                      );
                    }}
                  </For>
                  <Show when={favoriteFilter() === "only"}>
                    <span class="filter-pill pill-fav">Favorites</span>
                  </Show>
                  <Show when={favoriteFilter() === "exclude"}>
                    <span class="filter-pill pill-fav">No favorites</span>
                  </Show>
                  <Show when={includeArchived()}>
                    <span class="filter-pill">+Archived</span>
                  </Show>
                </div>
              </Show>
            </div>

            <div class="results-list">
              <Show when={totalResultCount() === 0 && searched() && !loading()}>
                <div class="empty-state">
                  <p class="empty-text">No results match your search criteria.</p>
                  <Show when={activeFilterCount() > 0}>
                    <button class="btn-reset-inline" onClick={resetFilters} title="Reset all filters and show all items">Reset filters</button>
                  </Show>
                </div>
              </Show>

              <For each={unifiedResults()}>
                {(result) => (
                  <>
                    <Show when={result.kind === "prompt"}>
                      {(() => {
                        const prompt = (result as { kind: "prompt"; item: EnrichedPrompt }).item;
                        return (
                          <div class="glass-card result-card">
                            <div class="card-header">
                              <h4 class="card-title">{prompt.title}</h4>
                              <button class="copy-btn" onClick={() => handleCopy(prompt)} title="Copy to clipboard">
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                                  <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" stroke-width="1.3"/>
                                  <path d="M3 11V3a1.5 1.5 0 011.5-1.5H11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                                </svg>
                                Copy
                              </button>
                            </div>
                            <p class="card-preview">{previewContent(prompt.content)}</p>
                            <div class="card-meta">
                              <span class="card-date">{formatTimestamp(prompt.updated_at)}</span>
                              <Show when={prompt.language}>
                                <span class="card-badge lang-badge">{prompt.language}</span>
                              </Show>
                              <Show when={prompt.is_favorite}>
                                <span class="card-badge favorite-badge">Favorite</span>
                              </Show>
                              <Show when={prompt.is_archived}>
                                <span class="card-badge archive-badge">Archived</span>
                              </Show>
                            </div>
                          </div>
                        );
                      })()}
                    </Show>
                    <Show when={result.kind === "chain"}>
                      {(() => {
                        const chain = (result as { kind: "chain"; item: Chain }).item;
                        return (
                          <div class="glass-card result-card">
                            <div class="card-header">
                              <h4 class="card-title">{chain.title}</h4>
                              <button class="copy-btn" onClick={() => handleCopyChain(chain)} title="Copy chain to clipboard">
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                                  <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" stroke-width="1.3"/>
                                  <path d="M3 11V3a1.5 1.5 0 011.5-1.5H11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                                </svg>
                                Copy
                              </button>
                            </div>
                            <p class="card-preview">{chain.description ? previewContent(chain.description) : "Prompt chain"}</p>
                            <div class="card-meta">
                              <span class="card-badge chain-badge">Chain</span>
                              <span class="card-date">{formatTimestamp(chain.updated_at)}</span>
                              <Show when={chain.is_favorite}>
                                <span class="card-badge favorite-badge">Favorite</span>
                              </Show>
                            </div>
                          </div>
                        );
                      })()}
                    </Show>
                    <Show when={result.kind === "script"}>
                      {(() => {
                        const script = (result as { kind: "script"; item: Script }).item;
                        return (
                          <div class="glass-card result-card">
                            <div class="card-header">
                              <h4 class="card-title">{script.title}</h4>
                              <button class="copy-btn" onClick={() => handleCopyScript(script)} title="Copy script to clipboard">
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                                  <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" stroke-width="1.3"/>
                                  <path d="M3 11V3a1.5 1.5 0 011.5-1.5H11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                                </svg>
                                Copy
                              </button>
                            </div>
                            <p class="card-preview">{previewContent(script.content)}</p>
                            <div class="card-meta">
                              <span class="card-badge script-badge">Script</span>
                              <span class="card-badge lang-badge">{script.script_language}</span>
                              <span class="card-date">{formatTimestamp(script.updated_at)}</span>
                              <Show when={script.is_favorite}>
                                <span class="card-badge favorite-badge">Favorite</span>
                              </Show>
                              <Show when={script.is_archived}>
                                <span class="card-badge archive-badge">Archived</span>
                              </Show>
                            </div>
                          </div>
                        );
                      })()}
                    </Show>
                  </>
                )}
              </For>
            </div>

            {/* Clipboard History */}
            <div class="history-section">
              <button class="history-toggle" onClick={loadHistory} title="Show/hide clipboard copy history">
                <svg
                  class={`history-chevron${historyExpanded() ? " expanded" : ""}`}
                  width="12" height="12" viewBox="0 0 12 12"
                >
                  <path d="M3 4.5l3 3 3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
                </svg>
                Clipboard History
              </button>

              <Show when={historyExpanded()}>
                <div class="history-list">
                  <Show when={clipboardHistory().length === 0}>
                    <p class="empty-text">No clipboard history entries.</p>
                  </Show>
                  <For each={clipboardHistory()}>
                    {(entry) => (
                      <div class="history-item">
                        <div class="history-meta">
                          <span class="history-title">{entry.prompt_title}</span>
                          <span class="history-time">{formatTimestamp(entry.copied_at)}</span>
                        </div>
                        <p class="history-content">{previewContent(entry.content)}</p>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </div>
        }
      />
    </div>
  );
};

export default ClipboardTab;
