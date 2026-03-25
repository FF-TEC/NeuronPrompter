import { Component, Show, For, createSignal, createEffect } from "solid-js";
import type { Prompt, Tag, Category, Collection, NavFilter } from "../api/types";
import { formatDate, previewContent } from "../utils";
import "./PromptList.css";

interface PromptListProps {
  prompts: Prompt[];
  activePromptId: number | null;
  tags: Tag[];
  categories: Category[];
  collections: Collection[];
  activeFilter: NavFilter;
  searchQuery: string;
  onSelect: (promptId: number) => void;
  onToggleFavorite: (promptId: number, isFavorite: boolean) => void;
  onFilterChange: (filter: NavFilter) => void;
  onSearchChange: (query: string) => void;
  onNewPrompt: () => void;
}

function isFilterActive(active: NavFilter, filter: NavFilter): boolean {
  if (active.kind !== filter.kind) return false;
  if ("id" in filter && "id" in active) return filter.id === active.id;
  return true;
}

const PromptList: Component<PromptListProps> = (props) => {
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
    <div class="prompt-list-panel">
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
            placeholder="Search prompts..."
            aria-label="Search prompts"
            data-tooltip="Search prompts by title and content (Ctrl+F)"
          />
        </div>
      </div>

      {/* New prompt button */}
      <button
        class="new-item-btn"
        onClick={props.onNewPrompt}
        data-tooltip="New Prompt (Ctrl+N)"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
          <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        New Prompt
      </button>

      {/* Filter chips */}
      <div class="filter-chips">
        <button
          class={`filter-chip${isFilterActive(props.activeFilter, { kind: "all" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "all" })}
          data-tooltip="Show all prompts"
        >All</button>
        <button
          class={`filter-chip filter-chip-fav${isFilterActive(props.activeFilter, { kind: "favorites" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "favorites" })}
          data-tooltip="Show only favorite prompts"
        >Favorites</button>
        <button
          class={`filter-chip filter-chip-archive${isFilterActive(props.activeFilter, { kind: "archive" }) ? " active" : ""}`}
          onClick={() => props.onFilterChange({ kind: "archive" })}
          data-tooltip="Show archived prompts"
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

      {/* Prompt list */}
      <div class="prompt-scroll">
        <Show
          when={props.prompts.length > 0}
          fallback={
            <div class="empty-state">
              <p class="empty-title">No prompts found</p>
              <p class="empty-hint">Create a new prompt with Ctrl+N</p>
            </div>
          }
        >
          <For each={props.prompts}>
            {(prompt) => (
              <div
                class={`prompt-item${prompt.id === props.activePromptId ? " active" : ""}`}
                onClick={() => props.onSelect(prompt.id)}
                onKeyDown={(e: KeyboardEvent) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    props.onSelect(prompt.id);
                  }
                }}
                role="button"
                tabindex={0}
              >
                <div class="prompt-item-header">
                  <span class="prompt-title">{prompt.title}</span>
                  <button
                    class={`favorite-btn${prompt.is_favorite ? " is-favorite" : ""}`}
                    onClick={(e: MouseEvent) => {
                      e.stopPropagation();
                      props.onToggleFavorite(prompt.id, !prompt.is_favorite);
                    }}
                    attr:data-tooltip={prompt.is_favorite ? "Remove from favorites" : "Add to favorites"}
                    aria-label={prompt.is_favorite ? "Remove from favorites" : "Add to favorites"}
                  >
                    <svg width="14" height="14" viewBox="0 0 16 16" fill={prompt.is_favorite ? "currentColor" : "none"}>
                      <path d="M8 2l1.5 3.5L13 6l-2.5 2.5.5 3.5L8 10.5 4.5 12l.5-3.5L2.5 6l3.5-.5z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" />
                    </svg>
                  </button>
                </div>
                <p class="prompt-preview">{previewContent(prompt.content)}</p>
                <div class="prompt-meta">
                  <span class="prompt-date">{formatDate(prompt.updated_at)}</span>
                  <Show when={prompt.current_version > 1}>
                    <span class="version-badge">v{prompt.current_version}</span>
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

export default PromptList;
