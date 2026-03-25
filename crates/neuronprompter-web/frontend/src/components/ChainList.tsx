import { Component, Show, For, createSignal, createEffect } from "solid-js";
import type { Chain, Tag, Category, Collection, NavFilter } from "../api/types";
import { formatDate, previewContent } from "../utils";
import "./ChainList.css";

interface ChainListProps {
  chains: Chain[];
  activeChainId: number | null;
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
  activeFilter: NavFilter;
  searchQuery: string;
  onSelectChain: (chainId: number) => void;
  onToggleFavorite: (chainId: number, isFavorite: boolean) => void;
  onFilterChange: (filter: NavFilter) => void;
  onSearchChange: (query: string) => void;
  onNewChain: () => void;
}

/**
 * Compares two NavFilter values for equality.
 *
 * @param active - The currently active filter.
 * @param filter - The filter to compare against.
 * @returns True when both filters represent the same selection.
 */
function isFilterActive(active: NavFilter, filter: NavFilter): boolean {
  if (active.kind !== filter.kind) return false;
  if ("id" in filter && "id" in active) return filter.id === active.id;
  return true;
}

/**
 * Extracts a short preview string from a chain's description.
 *
 * @param chain - The chain entity.
 * @returns A truncated description or a fallback label.
 */
function previewChain(chain: Chain): string {
  if (chain.description && chain.description.length > 0) {
    return previewContent(chain.description);
  }
  return "Prompt chain";
}

const ChainList: Component<ChainListProps> = (props) => {
  const [localQuery, setLocalQuery] = createSignal("");

  createEffect(() => {
    setLocalQuery(props.searchQuery);
  });

  const handleSearchInput = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    setLocalQuery(value);
    props.onSearchChange(value);
  };

  return (
    <div class="chain-list-panel">
      {/* Search bar */}
      <div class="chain-search-bar">
        <div class="chain-search-input-wrapper">
          <svg class="chain-search-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
            <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.2" />
            <path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
          </svg>
          <input
            type="text"
            class="chain-search-input"
            value={localQuery()}
            onInput={handleSearchInput}
            placeholder="Search chains..."
            aria-label="Search chains"
            data-tooltip="Search chains by title and description"
          />
        </div>
      </div>

      {/* New chain button */}
      <button
        class="new-item-btn"
        onClick={props.onNewChain}
        data-tooltip="New Chain"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
          <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        New Chain
      </button>

      {/* Filter chips */}
      <div class="chain-filter-chips">
        <button
          class={`chain-filter-chip${isFilterActive(props.activeFilter, { kind: "all" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "all" })}
          data-tooltip="Show all chains"
        >All</button>
        <button
          class={`chain-filter-chip chain-filter-chip-fav${isFilterActive(props.activeFilter, { kind: "favorites" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "favorites" })}
          data-tooltip="Show only favorite chains"
        >Favorites</button>
        <button
          class={`chain-filter-chip chain-filter-chip-archive${isFilterActive(props.activeFilter, { kind: "archive" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "archive" })}
          data-tooltip="Show archived chains"
        >Archive</button>

        <For each={props.tags}>
          {(tag) => (
            <button
              class={`chain-filter-chip chain-filter-chip-tag${isFilterActive(props.activeFilter, { kind: "tag", id: tag.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "tag", id: tag.id })}
              attr:data-tooltip={"Filter by tag: " + tag.name}
            >{tag.name}</button>
          )}
        </For>

        <For each={props.categories}>
          {(cat) => (
            <button
              class={`chain-filter-chip chain-filter-chip-category${isFilterActive(props.activeFilter, { kind: "category", id: cat.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "category", id: cat.id })}
              attr:data-tooltip={"Filter by category: " + cat.name}
            >{cat.name}</button>
          )}
        </For>

        <For each={props.collections}>
          {(col) => (
            <button
              class={`chain-filter-chip chain-filter-chip-collection${isFilterActive(props.activeFilter, { kind: "collection", id: col.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "collection", id: col.id })}
              attr:data-tooltip={"Filter by collection: " + col.name}
            >{col.name}</button>
          )}
        </For>
      </div>

      {/* Chain list */}
      <div class="chain-scroll">
        <Show
          when={props.chains.length > 0}
          fallback={
            <div class="chain-empty-state">
              <p class="chain-empty-title">No chains found</p>
              <p class="chain-empty-hint">Create your first chain.</p>
            </div>
          }
        >
          <For each={props.chains}>
            {(chain) => (
              <div
                class={`chain-item${chain.id === props.activeChainId ? " active" : ""}`}
                onClick={() => props.onSelectChain(chain.id)}
                onKeyDown={(e: KeyboardEvent) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    props.onSelectChain(chain.id);
                  }
                }}
                role="button"
                tabindex={0}
              >
                <div class="chain-item-header">
                  <span class="chain-title">{chain.title}</span>
                  <button
                    class={`chain-favorite-btn${chain.is_favorite ? " is-favorite" : ""}`}
                    onClick={(e: MouseEvent) => {
                      e.stopPropagation();
                      props.onToggleFavorite(chain.id, !chain.is_favorite);
                    }}
                    attr:data-tooltip={chain.is_favorite ? "Remove from favorites" : "Add to favorites"}
                    aria-label={chain.is_favorite ? "Remove from favorites" : "Add to favorites"}
                  >
                    <svg width="14" height="14" viewBox="0 0 16 16" fill={chain.is_favorite ? "currentColor" : "none"}>
                      <path d="M8 2l1.5 3.5L13 6l-2.5 2.5.5 3.5L8 10.5 4.5 12l.5-3.5L2.5 6l3.5-.5z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
                    </svg>
                  </button>
                </div>
                <div class="chain-badges">
                  <span class="chain-step-badge">Chain</span>
                </div>
                <p class="chain-preview">{previewChain(chain)}</p>
                <div class="chain-meta">
                  <span class="chain-date">{formatDate(chain.updated_at)}</span>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default ChainList;
