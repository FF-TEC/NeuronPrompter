import { logError } from "../utils/errors";
import { Component, Show, For, createSignal, createMemo, createEffect, on, onMount, onCleanup } from "solid-js";
import { api } from "../api/client";
import { actions } from "../stores/app";
import type { PromptWithAssociations, Tag, Category, Collection, User } from "../api/types";
import { arraysEqual } from "../utils";
import VersionPanel from "./VersionPanel";
import CopyToUserDropdown from "./ui/CopyToUserDropdown";
import "./PromptEditor.css";

interface PromptEditorProps {
  detail: PromptWithAssociations | null;
  allTags: Tag[];
  allCategories: Category[];
  allCollections: Collection[];
  activeUser: User | null;
  onSave: () => void;
  onDirtyChange: (dirty: boolean) => void;
  onCopy: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onArchiveToggle: (promptId: number, isArchived: boolean) => void;
  onVersionRestore: () => void;
  ollamaBaseUrl: string;
  ollamaModel: string | null;
  ollamaConnected: boolean;
}

interface Snapshot {
  title: string;
  content: string;
  description: string;
  notes: string;
  language: string;
  tagIds: number[];
  categoryIds: number[];
  collectionIds: number[];
}

const PromptEditor: Component<PromptEditorProps> = (props) => {
  const [title, setTitle] = createSignal("");
  const [content, setContent] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [language, setLanguage] = createSignal("");
  const [selectedTagIds, setSelectedTagIds] = createSignal<number[]>([]);
  const [selectedCategoryIds, setSelectedCategoryIds] = createSignal<number[]>([]);
  const [selectedCollectionIds, setSelectedCollectionIds] = createSignal<number[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [optimizing, setOptimizing] = createSignal(false);
  const [autofilling, setAutofilling] = createSignal(false);
  const [translating, setTranslating] = createSignal(false);
  const [targetLanguage, setTargetLanguage] = createSignal("English");
  const ollamaBusy = () => optimizing() || autofilling() || translating();
  const ollamaReady = () => props.ollamaConnected && !!props.ollamaModel;
  const [snapshot, setSnapshot] = createSignal<Snapshot>({
    title: "",
    content: "",
    description: "",
    notes: "",
    language: "",
    tagIds: [],
    categoryIds: [],
    collectionIds: [],
  });

  // Sync form fields when the active prompt changes.
  // Track primitive values (id + updated_at) instead of the store proxy reference,
  // because SolidJS store proxies keep the same reference even after reconcile().
  createEffect(
    on(
      () => props.detail ? `${props.detail.prompt.id}:${props.detail.prompt.updated_at}` : null,
      () => {
        const detail = props.detail;
        if (detail) {
          const p = detail.prompt;
          setTitle(p.title);
          setContent(p.content);
          setDescription(p.description ?? "");
          setNotes(p.notes ?? "");
          setLanguage(p.language ?? "");
          setSelectedTagIds(detail.tags.map((t) => t.id));
          setSelectedCategoryIds(detail.categories.map((c) => c.id));
          setSelectedCollectionIds(detail.collections.map((c) => c.id));
          setSnapshot({
            title: p.title,
            content: p.content,
            description: p.description ?? "",
            notes: p.notes ?? "",
            language: p.language ?? "",
            tagIds: detail.tags.map((t) => t.id),
            categoryIds: detail.categories.map((c) => c.id),
            collectionIds: detail.collections.map((c) => c.id),
          });
        }
      },
    ),
  );

  const isDirty = createMemo(() => {
    const snap = snapshot();
    return (
      title() !== snap.title ||
      content() !== snap.content ||
      description() !== snap.description ||
      notes() !== snap.notes ||
      language() !== snap.language ||
      !arraysEqual(selectedTagIds(), snap.tagIds) ||
      !arraysEqual(selectedCategoryIds(), snap.categoryIds) ||
      !arraysEqual(selectedCollectionIds(), snap.collectionIds)
    );
  });

  createEffect(() => {
    props.onDirtyChange(isDirty());
  });

  const handleSave = async () => {
    if (!props.detail || saving()) return;
    setSaving(true);
    try {
      await api.updatePrompt({
        prompt_id: props.detail.prompt.id,
        title: title(),
        content: content(),
        description: description() || null,
        notes: notes() || null,
        language: language() || null,
        tag_ids: selectedTagIds(),
        category_ids: selectedCategoryIds(),
        collection_ids: selectedCollectionIds(),
      });
      setSnapshot({
        title: title(),
        content: content(),
        description: description(),
        notes: notes(),
        language: language(),
        tagIds: [...selectedTagIds()],
        categoryIds: [...selectedCategoryIds()],
        collectionIds: [...selectedCollectionIds()],
      });
      props.onSave();
    } catch (err) {
      actions.addToast("error", "Save Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  // Register this editor's save handler so Ctrl+S works via the store.
  onMount(() => {
    actions.setSaveHandler(() => handleSave());
  });
  onCleanup(() => {
    actions.setSaveHandler(null);
  });

  const toggleTag = (tagId: number) => {
    if (selectedTagIds().includes(tagId)) {
      setSelectedTagIds(selectedTagIds().filter((id) => id !== tagId));
    } else {
      setSelectedTagIds([...selectedTagIds(), tagId]);
    }
  };

  const toggleCategory = (catId: number) => {
    if (selectedCategoryIds().includes(catId)) {
      setSelectedCategoryIds(selectedCategoryIds().filter((id) => id !== catId));
    } else {
      setSelectedCategoryIds([...selectedCategoryIds(), catId]);
    }
  };

  const toggleCollection = (colId: number) => {
    if (selectedCollectionIds().includes(colId)) {
      setSelectedCollectionIds(selectedCollectionIds().filter((id) => id !== colId));
    } else {
      setSelectedCollectionIds([...selectedCollectionIds(), colId]);
    }
  };

  const handleToggleArchive = async () => {
    if (!props.detail) return;
    const newValue = !props.detail.prompt.is_archived;
    try {
      await api.toggleArchive(props.detail.prompt.id, newValue);
      props.onArchiveToggle(props.detail.prompt.id, newValue);
    } catch (err) {
      logError("PromptEditor.toggleArchive", err);
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  };

  const handleOptimize = async () => {
    if (!props.ollamaModel || !content().trim()) return;
    setOptimizing(true);
    try {
      const improved = await api.ollamaImprove(props.ollamaBaseUrl, props.ollamaModel, content());
      setContent(improved);
    } catch (err) {
      logError("PromptEditor.optimize", err);
      actions.addToast("error", "Optimize Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setOptimizing(false);
    }
  };

  const handleAutofillAction = async () => {
    if (!props.ollamaModel || !content().trim()) return;
    setAutofilling(true);
    try {
      const meta = await api.ollamaAutofill(props.ollamaBaseUrl, props.ollamaModel, content());
      setDescription(meta.description);
      setLanguage(meta.language);
      if (title() === "Untitled Prompt" && meta.description) {
        setTitle(meta.description.slice(0, 60));
      }
    } catch (err) {
      logError("PromptEditor.autofill", err);
      actions.addToast("error", "Autofill Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setAutofilling(false);
    }
  };

  const handleTranslate = async () => {
    if (!props.ollamaModel || !content().trim()) return;
    setTranslating(true);
    try {
      const translated = await api.ollamaTranslate(
        props.ollamaBaseUrl,
        props.ollamaModel,
        content(),
        targetLanguage(),
      );
      setContent(translated);
    } catch (err) {
      logError("PromptEditor.translate", err);
      actions.addToast("error", "Translation Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setTranslating(false);
    }
  };

  return (
    <div class="editor">
      <Show
        when={props.detail}
        fallback={
          <div class="editor-empty">
            <p class="empty-title">No prompt selected</p>
            <p class="empty-hint">Select a prompt from the list or create a new one with Ctrl+N</p>
          </div>
        }
      >
        <div class="editor-content">
          {/* Title + Language row */}
          <div class="metadata-row">
            <div class="field-group field-group-grow-row">
              <label class="field-label" for="prompt-title">Title</label>
              <input
                id="prompt-title"
                type="text"
                class="field-input"
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
                placeholder="Prompt title"
                data-tooltip="Prompt title — displayed in the list"
              />
            </div>
            <div class="field-group field-group-lang">
              <label class="field-label" for="prompt-language">Language</label>
              <input
                id="prompt-language"
                type="text"
                class="field-input"
                value={language()}
                onInput={(e) => setLanguage(e.currentTarget.value)}
                placeholder="e.g., English"
                data-tooltip="Natural language of this prompt (e.g. en, de)"
              />
            </div>
          </div>

          {/* Content (main textarea) */}
          <div class="field-group field-group-grow">
            <div class="field-label-row">
              <label class="field-label" for="prompt-content">Content</label>
              <Show when={ollamaReady()}>
                <div class="ollama-inline-actions">
                  <button
                    class="ollama-inline-btn ollama-inline-btn-accent"
                    onClick={handleOptimize}
                    disabled={ollamaBusy() || !content().trim()}
                    data-tooltip="Optimize prompt text for clarity and effectiveness"
                  >
                    <Show
                      when={!optimizing()}
                      fallback={
                        <>
                          <div class="btn-spinner" />
                          Optimizing...
                        </>
                      }
                    >
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                        <path d="M8 1v4M8 11v4M1 8h4M11 8h4M3.5 3.5l2 2M10.5 10.5l2 2M3.5 12.5l2-2M10.5 5.5l2-2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
                      </svg>
                      Optimize
                    </Show>
                  </button>
                  <div class="ollama-inline-translate-group">
                    <button
                      class="ollama-inline-btn"
                      onClick={handleTranslate}
                      disabled={ollamaBusy() || !content().trim()}
                      data-tooltip="Translate the prompt content"
                    >
                      <Show
                        when={!translating()}
                        fallback={
                          <>
                            <div class="btn-spinner" />
                            Translating...
                          </>
                        }
                      >
                        <svg width="12" height="12" viewBox="0 0 14 14" fill="none">
                          <path d="M2 3h5M4.5 3v6M2 5.5c1 2 3 3.5 5 3.5M7 3c-1 2-3 3.5-5 3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
                          <path d="M8 11l1.5-4 1.5 4M8.5 10h2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
                        </svg>
                        Translate
                      </Show>
                    </button>
                    <input
                      type="text"
                      class="ollama-inline-lang-input"
                      value={targetLanguage()}
                      onInput={(e) => setTargetLanguage(e.currentTarget.value)}
                      placeholder="Language"
                      disabled={ollamaBusy()}
                      data-tooltip="Target language (e.g. English, German)"
                    />
                  </div>
                </div>
              </Show>
            </div>
            <textarea
              id="prompt-content"
              class="field-textarea"
              value={content()}
              onInput={(e) => setContent(e.currentTarget.value)}
              placeholder="Write your prompt content here..."
              data-tooltip="Main prompt content — supports {{variables}} for template substitution"
            />
          </div>

          {/* Description */}
          <div class="field-group">
            <div class="field-label-row">
              <label class="field-label" for="prompt-description">Description</label>
              <Show when={ollamaReady()}>
                <button
                  class="ollama-inline-btn ollama-inline-btn-autofill"
                  onClick={handleAutofillAction}
                  disabled={ollamaBusy() || !content().trim()}
                  data-tooltip="Auto-fill description, language, and title from prompt content"
                >
                  <Show
                    when={!autofilling()}
                    fallback={
                      <>
                        <div class="btn-spinner spinner-purple" />
                        Auto-filling...
                      </>
                    }
                  >
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                      <path d="M2 4h12M2 8h8M2 12h10" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
                      <circle cx="13" cy="11" r="2" stroke="currentColor" stroke-width="1.2" />
                    </svg>
                    Auto-fill
                  </Show>
                </button>
              </Show>
            </div>
            <textarea
              id="prompt-description"
              class="field-textarea field-textarea-sm"
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              placeholder="Describe what this prompt does..."
              data-tooltip="Short description for search and preview"
            />
          </div>

          {/* Notes */}
          <div class="field-group">
            <label class="field-label" for="prompt-notes">Notes</label>
            <textarea
              id="prompt-notes"
              class="field-textarea field-textarea-sm"
              value={notes()}
              onInput={(e) => setNotes(e.currentTarget.value)}
              placeholder="Personal notes..."
              data-tooltip="Private notes — not included when copying"
            />
          </div>

          {/* Tags */}
          <Show when={props.allTags.length > 0}>
            <div class="field-group">
              <span class="field-label">Tags</span>
              <div class="chip-container">
                <For each={props.allTags}>
                  {(tag) => (
                    <button
                      class={`chip chip-tag${selectedTagIds().includes(tag.id) ? " selected" : ""}`}
                      onClick={() => toggleTag(tag.id)}
                      attr:data-tooltip={selectedTagIds().includes(tag.id) ? "Remove tag: " + tag.name : "Add tag: " + tag.name}
                    >
                      {tag.name}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Categories */}
          <Show when={props.allCategories.length > 0}>
            <div class="field-group">
              <span class="field-label">Categories</span>
              <div class="chip-container">
                <For each={props.allCategories}>
                  {(cat) => (
                    <button
                      class={`chip chip-category${selectedCategoryIds().includes(cat.id) ? " selected" : ""}`}
                      onClick={() => toggleCategory(cat.id)}
                      attr:data-tooltip={selectedCategoryIds().includes(cat.id) ? "Remove category: " + cat.name : "Add category: " + cat.name}
                    >
                      {cat.name}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Collections */}
          <Show when={props.allCollections.length > 0}>
            <div class="field-group">
              <span class="field-label">Collections</span>
              <div class="chip-container">
                <For each={props.allCollections}>
                  {(col) => (
                    <button
                      class={`chip chip-collection${selectedCollectionIds().includes(col.id) ? " selected" : ""}`}
                      onClick={() => toggleCollection(col.id)}
                      attr:data-tooltip={selectedCollectionIds().includes(col.id) ? "Remove from collection: " + col.name : "Add to collection: " + col.name}
                    >
                      {col.name}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Version history */}
          <VersionPanel
            entityId={props.detail!.prompt.id}
            entityType="prompt"
            currentVersion={props.detail!.prompt.current_version}
            onRestore={props.onVersionRestore}
          />

          {/* Editor toolbar */}
          <div class="editor-toolbar">
            <button class="btn btn-primary" onClick={handleSave} disabled={!isDirty() || saving()} title="Save changes (Ctrl+S)">
              {saving() ? "Saving..." : "Save"}
            </button>
            <button class="btn btn-secondary" onClick={props.onCopy} title="Copy content to clipboard (Ctrl+Shift+C)">Copy</button>
            <button class="btn btn-secondary" onClick={props.onDuplicate} title="Create a copy of this prompt (Ctrl+D)">Duplicate</button>
            <CopyToUserDropdown
              entityType="prompt"
              entityId={props.detail!.prompt.id}
              entityTitle={props.detail!.prompt.title}
            />
            <button
              class="btn btn-secondary"
              onClick={handleToggleArchive}
              attr:data-tooltip={props.detail!.prompt.is_archived ? "Unarchive this prompt" : "Archive this prompt"}
            >
              {props.detail!.prompt.is_archived ? "Unarchive" : "Archive"}
            </button>
            <button class="btn btn-danger" onClick={props.onDelete} title="Permanently delete this prompt">Delete</button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default PromptEditor;
