import { Component, For, Show, createSignal, createMemo } from "solid-js";
import { state, actions } from "../../stores/app";
import { api } from "../../api/client";
import SplitPane from "../SplitPane";
import "./OrganizeTab.css";

/**
 * OrganizeTab: CRUD management for Tags, Categories, and Collections.
 *
 * SplitPane layout:
 * - Left: Create new items (3 add-forms with description)
 * - Right: Browse & manage existing items (grouped lists, inline rename, delete)
 */

const OrganizeTab: Component = () => {
  // -------------------------------------------------------------------------
  // Add-form state
  // -------------------------------------------------------------------------
  const [newTagName, setNewTagName] = createSignal("");
  const [newCategoryName, setNewCategoryName] = createSignal("");
  const [newCollectionName, setNewCollectionName] = createSignal("");

  // -------------------------------------------------------------------------
  // Inline rename state
  // -------------------------------------------------------------------------
  const [editingId, setEditingId] = createSignal<{ type: "tag" | "category" | "collection"; id: number } | null>(null);
  const [editName, setEditName] = createSignal("");

  // -------------------------------------------------------------------------
  // Totals
  // -------------------------------------------------------------------------
  const totalItems = createMemo(() => state.tags.length + state.categories.length + state.collections.length);

  // -------------------------------------------------------------------------
  // Refresh taxonomy from server
  // -------------------------------------------------------------------------
  async function refreshTaxonomy(): Promise<void> {
    if (!state.activeUser) return;
    const [tags, categories, collections] = await Promise.all([
      api.listTags(state.activeUser.id),
      api.listCategories(state.activeUser.id),
      api.listCollections(state.activeUser.id),
    ]);
    actions.setTags(tags);
    actions.setCategories(categories);
    actions.setCollections(collections);
  }

  // -------------------------------------------------------------------------
  // Create handlers
  // -------------------------------------------------------------------------
  async function handleCreateTag(): Promise<void> {
    if (!state.activeUser || !newTagName().trim()) return;
    try {
      await api.createTag(state.activeUser.id, newTagName().trim());
      setNewTagName("");
      await refreshTaxonomy();
      actions.addToast("success", "Created", "Tag created");
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleCreateCategory(): Promise<void> {
    if (!state.activeUser || !newCategoryName().trim()) return;
    try {
      await api.createCategory(state.activeUser.id, newCategoryName().trim());
      setNewCategoryName("");
      await refreshTaxonomy();
      actions.addToast("success", "Created", "Category created");
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleCreateCollection(): Promise<void> {
    if (!state.activeUser || !newCollectionName().trim()) return;
    try {
      await api.createCollection(state.activeUser.id, newCollectionName().trim());
      setNewCollectionName("");
      await refreshTaxonomy();
      actions.addToast("success", "Created", "Collection created");
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Delete handlers
  // -------------------------------------------------------------------------
  async function handleDelete(type: "tag" | "category" | "collection", id: number): Promise<void> {
    // M-56: Require explicit user confirmation before irreversible taxonomy deletion.
    if (!window.confirm(`Delete this ${type}? This cannot be undone.`)) return;
    try {
      if (type === "tag") await api.deleteTag(id);
      else if (type === "category") await api.deleteCategory(id);
      else await api.deleteCollection(id);
      await refreshTaxonomy();
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  // -------------------------------------------------------------------------
  // Inline rename
  // -------------------------------------------------------------------------
  function startEdit(type: "tag" | "category" | "collection", id: number, name: string): void {
    setEditingId({ type, id });
    setEditName(name);
  }

  async function commitEdit(): Promise<void> {
    const editing = editingId();
    if (!editing || !editName().trim()) {
      setEditingId(null);
      return;
    }
    try {
      if (editing.type === "tag") await api.renameTag(editing.id, editName().trim());
      else if (editing.type === "category") await api.renameCategory(editing.id, editName().trim());
      else await api.renameCollection(editing.id, editName().trim());
      await refreshTaxonomy();
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
    setEditingId(null);
  }

  function cancelEdit(): void {
    setEditingId(null);
  }

  function handleEditKeydown(e: KeyboardEvent): void {
    if (e.key === "Enter") { e.preventDefault(); commitEdit(); }
    else if (e.key === "Escape") { e.preventDefault(); cancelEdit(); }
  }

  function isEditing(type: string, id: number): boolean {
    const e = editingId();
    return e?.type === type && e?.id === id;
  }

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
  return (
    <div class="organize-tab">
      <SplitPane
        storageKey="organize"
        defaultRatio={0.35}
        minLeftPx={280}
        minRightPx={350}
        left={
          <div class="create-panel">
            <h3 class="panel-title">Create New</h3>
            <p class="panel-hint">Add tags, categories, and collections to organize your prompts.</p>

            {/* Create Tag */}
            <div class="create-card">
              <div class="create-header">
                <span class="create-dot tag-color" />
                <h4 class="create-label">Tag</h4>
              </div>
              <p class="create-desc">Keywords to label and find prompts quickly.</p>
              <form class="create-form" onSubmit={(e) => { e.preventDefault(); handleCreateTag(); }}>
                <input
                  type="text"
                  class="create-input"
                  value={newTagName()}
                  onInput={(e) => setNewTagName(e.currentTarget.value)}
                  placeholder="e.g. coding, writing, debug..."
                  data-tooltip="Enter a name for the new tag"
                />
                <button type="submit" class="btn-create" disabled={!newTagName().trim()} title="Create a new tag">
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                    <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                  Add
                </button>
              </form>
            </div>

            {/* Create Category */}
            <div class="create-card">
              <div class="create-header">
                <span class="create-dot category-color" />
                <h4 class="create-label">Category</h4>
              </div>
              <p class="create-desc">Broad groups to classify prompts by purpose.</p>
              <form class="create-form" onSubmit={(e) => { e.preventDefault(); handleCreateCategory(); }}>
                <input
                  type="text"
                  class="create-input"
                  value={newCategoryName()}
                  onInput={(e) => setNewCategoryName(e.currentTarget.value)}
                  placeholder="e.g. Development, Marketing..."
                  data-tooltip="Enter a name for the new category"
                />
                <button type="submit" class="btn-create" disabled={!newCategoryName().trim()} title="Create a new category">
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                    <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                  Add
                </button>
              </form>
            </div>

            {/* Create Collection */}
            <div class="create-card">
              <div class="create-header">
                <span class="create-dot collection-color" />
                <h4 class="create-label">Collection</h4>
              </div>
              <p class="create-desc">Named sets to bundle related prompts together.</p>
              <form class="create-form" onSubmit={(e) => { e.preventDefault(); handleCreateCollection(); }}>
                <input
                  type="text"
                  class="create-input"
                  value={newCollectionName()}
                  onInput={(e) => setNewCollectionName(e.currentTarget.value)}
                  placeholder="e.g. Client Project, Templates..."
                  data-tooltip="Enter a name for the new collection"
                />
                <button type="submit" class="btn-create" disabled={!newCollectionName().trim()} title="Create a new collection">
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                    <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                  Add
                </button>
              </form>
            </div>
          </div>
        }
        right={
          <div class="browse-panel">
            <div class="browse-header">
              <h3 class="panel-title">All Items</h3>
              <span class="total-badge">{totalItems()} total</span>
            </div>

            {/* Tags Section */}
            <div class="section">
              <div class="section-header">
                <span class="section-dot tag-color" />
                <h4 class="section-title">Tags</h4>
                <span class="section-count">{state.tags.length}</span>
              </div>
              <Show when={state.tags.length === 0}>
                <p class="empty-hint">No tags yet -- create one on the left.</p>
              </Show>
              <Show when={state.tags.length > 0}>
                <div class="item-list">
                  <For each={state.tags}>
                    {(tag) => (
                      <div class="item-row">
                        <span class="item-dot tag-color" />
                        <Show
                          when={isEditing("tag", tag.id)}
                          fallback={
                            <span class="item-name" onDblClick={() => startEdit("tag", tag.id, tag.name)} title="Double-click to rename">{tag.name}</span>
                          }
                        >
                          <input
                            class="edit-input"
                            value={editName()}
                            onInput={(e) => setEditName(e.currentTarget.value)}
                            onBlur={() => commitEdit()}
                            onKeyDown={handleEditKeydown}
                            autofocus
                          />
                        </Show>
                        <button class="btn-edit" onClick={() => startEdit("tag", tag.id, tag.name)} title="Rename">
                          <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M11.5 1.5l3 3L5 14H2v-3L11.5 1.5z" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
                        </button>
                        <button class="btn-delete" onClick={() => handleDelete("tag", tag.id)} title="Delete" aria-label={`Delete tag ${tag.name}`}>
                          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.2"/></svg>
                        </button>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            {/* Categories Section */}
            <div class="section">
              <div class="section-header">
                <span class="section-dot category-color" />
                <h4 class="section-title">Categories</h4>
                <span class="section-count">{state.categories.length}</span>
              </div>
              <Show when={state.categories.length === 0}>
                <p class="empty-hint">No categories yet -- create one on the left.</p>
              </Show>
              <Show when={state.categories.length > 0}>
                <div class="item-list">
                  <For each={state.categories}>
                    {(cat) => (
                      <div class="item-row">
                        <span class="item-dot category-color" />
                        <Show
                          when={isEditing("category", cat.id)}
                          fallback={
                            <span class="item-name" onDblClick={() => startEdit("category", cat.id, cat.name)} title="Double-click to rename">{cat.name}</span>
                          }
                        >
                          <input
                            class="edit-input"
                            value={editName()}
                            onInput={(e) => setEditName(e.currentTarget.value)}
                            onBlur={() => commitEdit()}
                            onKeyDown={handleEditKeydown}
                            autofocus
                          />
                        </Show>
                        <button class="btn-edit" onClick={() => startEdit("category", cat.id, cat.name)} title="Rename">
                          <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M11.5 1.5l3 3L5 14H2v-3L11.5 1.5z" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
                        </button>
                        <button class="btn-delete" onClick={() => handleDelete("category", cat.id)} title="Delete" aria-label={`Delete category ${cat.name}`}>
                          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.2"/></svg>
                        </button>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            {/* Collections Section */}
            <div class="section">
              <div class="section-header">
                <span class="section-dot collection-color" />
                <h4 class="section-title">Collections</h4>
                <span class="section-count">{state.collections.length}</span>
              </div>
              <Show when={state.collections.length === 0}>
                <p class="empty-hint">No collections yet -- create one on the left.</p>
              </Show>
              <Show when={state.collections.length > 0}>
                <div class="item-list">
                  <For each={state.collections}>
                    {(col) => (
                      <div class="item-row">
                        <span class="item-dot collection-color" />
                        <Show
                          when={isEditing("collection", col.id)}
                          fallback={
                            <span class="item-name" onDblClick={() => startEdit("collection", col.id, col.name)} title="Double-click to rename">{col.name}</span>
                          }
                        >
                          <input
                            class="edit-input"
                            value={editName()}
                            onInput={(e) => setEditName(e.currentTarget.value)}
                            onBlur={() => commitEdit()}
                            onKeyDown={handleEditKeydown}
                            autofocus
                          />
                        </Show>
                        <button class="btn-edit" onClick={() => startEdit("collection", col.id, col.name)} title="Rename">
                          <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M11.5 1.5l3 3L5 14H2v-3L11.5 1.5z" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>
                        </button>
                        <button class="btn-delete" onClick={() => handleDelete("collection", col.id)} title="Delete" aria-label={`Delete collection ${col.name}`}>
                          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.2"/></svg>
                        </button>
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

export default OrganizeTab;
