import { Component, Show, For, createSignal, createEffect } from "solid-js";
import type { Script, Tag, Category, Collection, NavFilter } from "../api/types";
import { formatDate, previewContent } from "../utils";
import "./ScriptList.css";

interface ScriptListProps {
  scripts: Script[];
  activeScriptId: number | null;
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
  activeFilter: NavFilter;
  searchQuery: string;
  onSelect: (scriptId: number) => void;
  onToggleFavorite: (scriptId: number, isFavorite: boolean) => void;
  onFilterChange: (filter: NavFilter) => void;
  onSearchChange: (query: string) => void;
  onNewScript: () => void;
}

function isFilterActive(active: NavFilter, filter: NavFilter): boolean {
  if (active.kind !== filter.kind) return false;
  if ("id" in filter && "id" in active) return filter.id === active.id;
  return true;
}

const ScriptList: Component<ScriptListProps> = (props) => {
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
    <div class="script-list-panel">
      {/* Search bar */}
      <div class="search-bar">
        <div class="search-input-wrapper">
          <svg class="search-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
            <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.2" />
            <path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
          </svg>
          <input
            type="text"
            class="search-input"
            value={localQuery()}
            onInput={handleSearchInput}
            placeholder="Search scripts..."
            aria-label="Search scripts"
            data-tooltip="Search scripts by title and content (Ctrl+F)"
          />
        </div>
      </div>

      {/* New script button */}
      <button
        class="new-item-btn"
        onClick={props.onNewScript}
        data-tooltip="New Script (Ctrl+N)"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
          <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        New Script
      </button>

      {/* Filter chips */}
      <div class="filter-chips">
        <button
          class={`filter-chip${isFilterActive(props.activeFilter, { kind: "all" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "all" })}
          data-tooltip="Show all scripts"
        >All</button>
        <button
          class={`filter-chip filter-chip-fav${isFilterActive(props.activeFilter, { kind: "favorites" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "favorites" })}
          data-tooltip="Show only favorite scripts"
        >Favorites</button>
        <button
          class={`filter-chip filter-chip-archive${isFilterActive(props.activeFilter, { kind: "archive" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "archive" })}
          data-tooltip="Show archived scripts"
        >Archive</button>

        <For each={props.tags}>
          {(tag) => (
            <button
              class={`filter-chip filter-chip-tag${isFilterActive(props.activeFilter, { kind: "tag", id: tag.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "tag", id: tag.id })}
              attr:data-tooltip={"Filter by tag: " + tag.name}
            >{tag.name}</button>
          )}
        </For>

        <For each={props.categories}>
          {(cat) => (
            <button
              class={`filter-chip filter-chip-category${isFilterActive(props.activeFilter, { kind: "category", id: cat.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "category", id: cat.id })}
              attr:data-tooltip={"Filter by category: " + cat.name}
            >{cat.name}</button>
          )}
        </For>

        <For each={props.collections}>
          {(col) => (
            <button
              class={`filter-chip filter-chip-collection${isFilterActive(props.activeFilter, { kind: "collection", id: col.id }) ? " active" : ""}`}
              onClick={() => props.onFilterChange({ kind: "collection", id: col.id })}
              attr:data-tooltip={"Filter by collection: " + col.name}
            >{col.name}</button>
          )}
        </For>
      </div>

      {/* Script list */}
      <div class="script-scroll">
        <Show
          when={props.scripts.length > 0}
          fallback={
            <div class="empty-state">
              <p class="empty-title">No scripts found</p>
              <p class="empty-hint">Create a new script with Ctrl+N</p>
            </div>
          }
        >
          <For each={props.scripts}>
            {(script) => (
              <div
                class={`script-item${script.id === props.activeScriptId ? " active" : ""}`}
                onClick={() => props.onSelect(script.id)}
                onKeyDown={(e: KeyboardEvent) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    props.onSelect(script.id);
                  }
                }}
                role="button"
                tabindex={0}
              >
                <div class="script-item-header">
                  <div class="script-title-row">
                    <span class="script-title">{script.title}</span>
                    <Show when={script.is_synced}>
                      <span class="sync-badge" title={script.source_path ?? "Synced"}>
                        <svg width="10" height="10" viewBox="0 0 16 16" fill="none">
                          <path d="M2 8a6 6 0 0111.2-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                          <path d="M14 8a6 6 0 01-11.2 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                        </svg>
                      </span>
                    </Show>
                    <Show when={script.script_language && script.script_language !== "text"}>
                      <span class="lang-badge">{script.script_language}</span>
                    </Show>
                  </div>
                  <button
                    class={`favorite-btn${script.is_favorite ? " is-favorite" : ""}`}
                    onClick={(e: MouseEvent) => {
                      e.stopPropagation();
                      props.onToggleFavorite(script.id, !script.is_favorite);
                    }}
                    attr:data-tooltip={script.is_favorite ? "Remove from favorites" : "Add to favorites"}
                    aria-label={script.is_favorite ? "Remove from favorites" : "Add to favorites"}
                  >
                    <svg width="14" height="14" viewBox="0 0 16 16" fill={script.is_favorite ? "currentColor" : "none"}>
                      <path d="M8 2l1.5 3.5L13 6l-2.5 2.5.5 3.5L8 10.5 4.5 12l.5-3.5L2.5 6l3.5-.5z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
                    </svg>
                  </button>
                </div>
                <p class="script-preview">{previewContent(script.content)}</p>
                <div class="script-meta">
                  <span class="script-date">{formatDate(script.updated_at)}</span>
                  <Show when={script.current_version > 1}>
                    <span class="version-badge">v{script.current_version}</span>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default ScriptList;
