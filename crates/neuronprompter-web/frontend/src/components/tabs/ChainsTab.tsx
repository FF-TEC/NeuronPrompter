import { logError } from "../../utils/errors";
import { Component, createSignal, onCleanup } from "solid-js";
import { state, actions, guardNavigation } from "../../stores/app";
import { api } from "../../api/client";
import type { NavFilter, ChainFilter } from "../../api/types";
import SplitPane from "../SplitPane";
import ChainEditor from "../ChainEditor";
import ChainList from "../ChainList";
import "./ChainsTab.css";

/**
 * ChainsTab: wrapper composing SplitPane with ChainEditor (left)
 * and ChainList (right). Orchestrates all chain-related actions.
 */
const ChainsTab: Component = () => {
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let selectCounter = 0;

  onCleanup(() => { if (debounceTimer) clearTimeout(debounceTimer); });

  const [navFilter, setNavFilter] = createSignal<NavFilter>({ kind: "all" });

  /**
   * Converts a NavFilter discriminated union into a ChainFilter payload
   * suitable for the listChains API call.
   *
   * @param filter - The active navigation filter.
   * @returns A ChainFilter with the corresponding query parameter set.
   */
  const buildFilter = (filter: NavFilter): ChainFilter => {
    const base: ChainFilter = { user_id: state.activeUser?.id };
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

  /** Reload chains from the API using the current filter. */
  const reloadChains = async () => {
    if (!state.activeUser) return;
    try {
      const chainList = (await api.listChains(buildFilter(navFilter()))).items;
      actions.setChains(chainList);
    } catch (e) {
      logError("ChainsTab.reloadChains", e);
      actions.addToast("error", "Reload Failed", e instanceof Error ? e.message : String(e));
    }
  };

  /** Select a chain and load its full detail. */
  const handleSelectChain = async (chainId: number) => {
    if (chainId === state.activeChainId) return;
    const result = await guardNavigation();
    if (result === "cancel") return;
    if (result === "save" && state.saveHandler) {
      try { await state.saveHandler(); } catch { return; }
    }
    actions.setActiveChainId(chainId);
    const myCounter = ++selectCounter;
    try {
      const detail = await api.getChain(chainId);
      if (selectCounter !== myCounter) return; // stale response
      actions.setActiveChainDetail(detail);
    } catch (e) {
      if (selectCounter !== myCounter) return;
      logError("ChainsTab.loadChain", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  /**
   * Applies a new navigation filter and reloads the chain list.
   *
   * @param filter - The filter selected by the user via a chip click.
   */
  const handleFilterChange = async (filter: NavFilter) => {
    setNavFilter(filter);
    if (!state.activeUser) return;
    try {
      const chains = (await api.listChains(buildFilter(filter))).items;
      actions.setChains(chains);
    } catch (e) {
      logError("ChainsTab.filterChains", e);
      actions.addToast("error", "Filter Failed", e instanceof Error ? e.message : String(e));
    }
  };

  /**
   * Handles search input with 300ms debounce. Empty queries restore the
   * current filter immediately; non-empty queries hit the search endpoint.
   *
   * @param query - The raw search string from the input field.
   */
  const handleSearchChange = async (query: string) => {
    actions.setSearchQuery(query);
    if (!state.activeUser) return;
    if (!query.trim()) {
      if (debounceTimer) clearTimeout(debounceTimer);
      await reloadChains();
      return;
    }
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      try {
        const chains = await api.searchChains(state.activeUser!.id, query);
        actions.setChains(chains);
      } catch {
        // Keep current list on error
      }
    }, 300);
  };

  /**
   * Toggles the favorite state of a chain and refreshes the list
   * respecting the current filter.
   *
   * @param chainId - The chain to toggle.
   * @param isFavorite - The desired favorite state.
   */
  const handleToggleFavorite = async (chainId: number, isFavorite: boolean) => {
    try {
      await api.toggleChainFavorite(chainId, isFavorite);
      const chains = state.chains.map((c) =>
        c.id === chainId ? { ...c, is_favorite: isFavorite } : c,
      );
      actions.setChains(chains);
    } catch (e) {
      logError("ChainsTab.toggleFavorite", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  /** Create a new chain with a single placeholder. */
  const handleNewChain = async () => {
    if (!state.activeUser) return;

    // Need at least one prompt or script to create a chain.
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
      await reloadChains();
      await handleSelectChain(chain.id);
      actions.addToast("success", "Chain Created", `"${chain.title}" created`);
    } catch (e) {
      logError("ChainsTab.createChain", e);
      actions.addToast("error", "Error", e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div class="chains-tab">
      <SplitPane
        storageKey="chains"
        defaultRatio={0.55}
        minLeftPx={420}
        minRightPx={280}
        left={<ChainEditor />}
        right={
          <ChainList
            chains={state.chains}
            activeChainId={state.activeChainId}
            tags={state.tags}
            categories={state.categories}
            collections={state.collections}
            activeFilter={navFilter()}
            searchQuery={state.searchQuery}
            onSelectChain={handleSelectChain}
            onToggleFavorite={handleToggleFavorite}
            onFilterChange={handleFilterChange}
            onSearchChange={handleSearchChange}
            onNewChain={handleNewChain}
          />
        }
      />
    </div>
  );
};

export default ChainsTab;
