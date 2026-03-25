import { logError } from "../utils/errors";
import { Component, Show, For, createSignal, createMemo, createEffect, on, onMount, onCleanup } from "solid-js";
import { api } from "../api/client";
import { actions } from "../stores/app";
import type { ScriptWithAssociations, Tag, Category, Collection, User, UpdateScript } from "../api/types";
import { arraysEqual } from "../utils";
import VersionPanel from "./VersionPanel";
import CopyToUserDropdown from "./ui/CopyToUserDropdown";
import "./ScriptEditor.css";

interface ScriptEditorProps {
  detail: ScriptWithAssociations | null;
  allTags: Tag[];
  allCategories: Category[];
  allCollections: Collection[];
  activeUser: User | null;
  onSave: () => void;
  onDirtyChange: (dirty: boolean) => void;
  onCopy: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onArchiveToggle: (scriptId: number, isArchived: boolean) => void;
  onVersionRestore: () => void;
  onImported?: (scriptId: number) => void;
}

/** Extension-to-language mapping for auto-detection. */
const EXTENSION_MAP: Record<string, string> = {
  py: "python", js: "javascript", ts: "typescript", tsx: "typescript",
  jsx: "javascript", sh: "bash", rs: "rust", go: "go", rb: "ruby",
  java: "java", c: "c", cpp: "cpp", cc: "cpp", h: "c", hpp: "cpp",
  cs: "csharp", swift: "swift", kt: "kotlin", kts: "kotlin",
  lua: "lua", php: "php", sql: "sql", r: "r", pl: "perl",
  ps1: "powershell", bat: "batch", cmd: "batch", zsh: "zsh",
  fish: "fish", html: "html", htm: "html", css: "css", scss: "scss",
  sass: "sass", less: "less", yaml: "yaml", yml: "yaml", json: "json",
  xml: "xml", toml: "toml", md: "markdown", markdown: "markdown",
  dockerfile: "dockerfile", makefile: "makefile", cmake: "cmake",
  gradle: "gradle", tf: "terraform", hcl: "hcl", vue: "vue",
  svelte: "svelte", dart: "dart", ex: "elixir", exs: "elixir",
  erl: "erlang", hs: "haskell", ml: "ocaml", scala: "scala",
  clj: "clojure", nim: "nim", zig: "zig", v: "v", proto: "protobuf",
  graphql: "graphql", gql: "graphql",
};

function detectLanguage(filename: string): string {
  const lower = filename.toLowerCase();
  const specialFiles: Record<string, string> = {
    dockerfile: "dockerfile", makefile: "makefile",
    cmakelists: "cmake", gemfile: "ruby", rakefile: "ruby",
    vagrantfile: "ruby", justfile: "just",
  };
  const baseName = lower.replace(/\.[^/.]+$/, "");
  if (specialFiles[baseName]) return specialFiles[baseName];
  const ext = lower.split(".").pop() ?? "";
  return EXTENSION_MAP[ext] ?? "";
}

interface Snapshot {
  title: string;
  content: string;
  description: string;
  notes: string;
  scriptLanguage: string;
  tagIds: number[];
  categoryIds: number[];
  collectionIds: number[];
}

const ScriptEditor: Component<ScriptEditorProps> = (props) => {
  const [title, setTitle] = createSignal("");
  const [content, setContent] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [scriptLanguage, setScriptLanguage] = createSignal("text");
  const [selectedTagIds, setSelectedTagIds] = createSignal<number[]>([]);
  const [selectedCategoryIds, setSelectedCategoryIds] = createSignal<number[]>([]);
  const [selectedCollectionIds, setSelectedCollectionIds] = createSignal<number[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [snapshot, setSnapshot] = createSignal<Snapshot>({
    title: "", content: "", description: "", notes: "",
    scriptLanguage: "text", tagIds: [], categoryIds: [], collectionIds: [],
  });

  // Import form state
  const [showImportForm, setShowImportForm] = createSignal(false);
  const [importPath, setImportPath] = createSignal("");
  const [importSynced, setImportSynced] = createSignal(false);
  const [importing, setImporting] = createSignal(false);

  let fileInputRef: HTMLInputElement | undefined;

  /** Whether this script is synced with a source file. */
  const isSynced = createMemo(() => props.detail?.script.is_synced ?? false);
  const sourcePath = createMemo(() => props.detail?.script.source_path ?? null);

  const languageDetected = createMemo(() => detectLanguage(title()) !== "");
  const effectiveLanguage = createMemo(() => detectLanguage(title()) || scriptLanguage());

  createEffect(on(title, (t) => {
    const detected = detectLanguage(t);
    if (detected) setScriptLanguage(detected);
  }));

  // Track primitive values (id + updated_at) to detect switches reliably,
  // because SolidJS store proxies keep the same reference even after reconcile().
  createEffect(on(
    () => props.detail ? `${props.detail.script.id}:${props.detail.script.updated_at}` : null,
    () => {
    const detail = props.detail;
    if (detail) {
      const s = detail.script;
      setTitle(s.title);
      setContent(s.content);
      setDescription(s.description ?? "");
      setNotes(s.notes ?? "");
      setScriptLanguage(s.script_language);
      setSelectedTagIds(detail.tags.map((t) => t.id));
      setSelectedCategoryIds(detail.categories.map((c) => c.id));
      setSelectedCollectionIds(detail.collections.map((c) => c.id));
      setSnapshot({
        title: s.title, content: s.content,
        description: s.description ?? "", notes: s.notes ?? "",
        scriptLanguage: s.script_language,
        tagIds: detail.tags.map((t) => t.id),
        categoryIds: detail.categories.map((c) => c.id),
        collectionIds: detail.collections.map((c) => c.id),
      });
      // Reset import form when switching scripts
      setShowImportForm(false);
    }
  }));

  const isDirty = createMemo(() => {
    const snap = snapshot();
    if (isSynced()) {
      // When synced, only metadata changes count as dirty
      return (
        description() !== snap.description ||
        notes() !== snap.notes ||
        !arraysEqual(selectedTagIds(), snap.tagIds) ||
        !arraysEqual(selectedCategoryIds(), snap.categoryIds) ||
        !arraysEqual(selectedCollectionIds(), snap.collectionIds)
      );
    }
    return (
      title() !== snap.title ||
      content() !== snap.content ||
      description() !== snap.description ||
      notes() !== snap.notes ||
      effectiveLanguage() !== snap.scriptLanguage ||
      !arraysEqual(selectedTagIds(), snap.tagIds) ||
      !arraysEqual(selectedCategoryIds(), snap.categoryIds) ||
      !arraysEqual(selectedCollectionIds(), snap.collectionIds)
    );
  });

  createEffect(() => { props.onDirtyChange(isDirty()); });

  const handleSave = async () => {
    if (!props.detail || saving()) return;
    setSaving(true);
    try {
      const lang = effectiveLanguage();
      const update: UpdateScript = {
        script_id: props.detail.script.id,
        description: description() || null,
        notes: notes() || null,
        tag_ids: selectedTagIds(),
        category_ids: selectedCategoryIds(),
        collection_ids: selectedCollectionIds(),
      };
      // Only include content/title/language when NOT synced
      if (!isSynced()) {
        update.title = title();
        update.content = content();
        update.script_language = lang;
      }
      await api.updateScript(update);
      setSnapshot({
        title: title(), content: content(),
        description: description(), notes: notes(),
        scriptLanguage: lang,
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
    setSelectedTagIds(selectedTagIds().includes(tagId)
      ? selectedTagIds().filter((id) => id !== tagId)
      : [...selectedTagIds(), tagId]);
  };
  const toggleCategory = (catId: number) => {
    setSelectedCategoryIds(selectedCategoryIds().includes(catId)
      ? selectedCategoryIds().filter((id) => id !== catId)
      : [...selectedCategoryIds(), catId]);
  };
  const toggleCollection = (colId: number) => {
    setSelectedCollectionIds(selectedCollectionIds().includes(colId)
      ? selectedCollectionIds().filter((id) => id !== colId)
      : [...selectedCollectionIds(), colId]);
  };

  const handleToggleArchive = async () => {
    if (!props.detail) return;
    const newValue = !props.detail.script.is_archived;
    try {
      await api.toggleScriptArchive(props.detail.script.id, newValue);
      props.onArchiveToggle(props.detail.script.id, newValue);
    } catch (err) {
      logError("ScriptEditor.toggleArchive", err);
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  };

  /** Detach sync: clear source_path and is_synced, make content editable. */
  const handleDetach = async () => {
    if (!props.detail) return;
    try {
      await api.updateScript({
        script_id: props.detail.script.id,
        source_path: null,
        is_synced: false,
      });
      actions.addToast("info", "Detached", "Script is no longer synced with the source file");
      props.onSave(); // triggers reload
    } catch (err) {
      logError("ScriptEditor.detach", err);
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  };

  /** Import file from filesystem via backend. */
  const handleImportSubmit = async () => {
    if (!props.activeUser || !importPath().trim() || importing()) return;
    setImporting(true);
    try {
      const script = await api.importScriptFile(
        props.activeUser.id,
        importPath().trim(),
        importSynced(),
      );
      actions.addToast("success", "Imported", `"${script.title}" imported successfully`);
      setShowImportForm(false);
      setImportPath("");
      setImportSynced(false);
      props.onImported?.(script.id);
    } catch (err: unknown) {
      const apiErr = err as { body?: string };
      const msg = apiErr?.body ? (() => { try { return JSON.parse(apiErr.body).message; } catch { return apiErr.body; } })() : (err instanceof Error ? err.message : String(err));
      actions.addToast("error", "Import Failed", msg);
    } finally {
      setImporting(false);
    }
  };

  /** Quick import via browser file picker (non-synced, fills editor). */
  const handleFileImport = async (e: Event) => {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    if (file.size > 1024 * 1024) {
      actions.addToast("error", "File too large", "Maximum file size is 1 MB");
      input.value = ""; return;
    }
    const text = await file.text();
    if (text.includes("\0")) {
      actions.addToast("error", "Binary file", "Cannot import binary files");
      input.value = ""; return;
    }
    setTitle(file.name);
    setContent(text);
    if (!detectLanguage(file.name)) setScriptLanguage("text");
    input.value = "";
  };

  return (
    <div class="editor">
      <Show
        when={props.detail}
        fallback={
          <div class="editor-empty">
            <p class="empty-title">No script selected</p>
            <p class="empty-hint">Create or select a script to start editing</p>
          </div>
        }
      >
        <div class="editor-content">
          {/* Sync indicator bar */}
          <Show when={isSynced()}>
            <div class="sync-bar">
              <div class="sync-bar-info">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                  <path d="M2 8a6 6 0 0111.2-3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                  <path d="M14 8a6 6 0 01-11.2 3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                  <path d="M13 2v3h-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                  <path d="M3 14v-3h3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span class="sync-bar-label">Synced from</span>
                <span class="sync-bar-path" title={sourcePath() ?? ""}>{sourcePath()}</span>
              </div>
              <button class="detach-btn" onClick={handleDetach} title="Stop syncing and make content editable">
                Detach
              </button>
            </div>
          </Show>

          {/* Import actions (always visible at top) */}
          <Show when={!isSynced()}>
            <Show
              when={showImportForm()}
              fallback={
                <div class="import-actions-row">
                  <button class="import-action-btn" onClick={() => setShowImportForm(true)} title="Import script content from a file path on the server">
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                      <path d="M2 10v3a1 1 0 001 1h10a1 1 0 001-1v-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
                      <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                    Import from Path
                  </button>
                  <button class="import-action-btn" onClick={() => fileInputRef?.click()} title="Select a local file to import as script content">
                    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                      <path d="M4 2h8a1 1 0 011 1v10a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1z" stroke="currentColor" stroke-width="1.2"/>
                      <path d="M5.5 5h5M5.5 7.5h5M5.5 10h3" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>
                    </svg>
                    Quick Import
                  </button>
                </div>
              }
            >
              <div class="import-form">
                <div class="import-form-header">
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" style="vertical-align: -2px;">
                    <path d="M2 10v3a1 1 0 001 1h10a1 1 0 001-1v-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
                    <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                  <span class="field-label" style="display: inline; margin-left: 6px;">Import from Filesystem</span>
                </div>
                <div class="field-group">
                  <input
                    type="text"
                    class="field-input filename-input"
                    value={importPath()}
                    onInput={(e) => setImportPath(e.currentTarget.value)}
                    placeholder="/absolute/path/to/script.py"
                    onKeyDown={(e) => { if (e.key === "Enter") handleImportSubmit(); }}
                    data-tooltip="Absolute filesystem path to import from"
                  />
                </div>
                <label class="sync-checkbox" title="Automatically update script content when the source file changes">
                  <input
                    type="checkbox"
                    checked={importSynced()}
                    onChange={(e) => setImportSynced(e.currentTarget.checked)}
                  />
                  <span>Keep synced with source file</span>
                </label>
                <Show when={importSynced()}>
                  <p class="sync-hint">Content will auto-update on app start. Editing will be disabled.</p>
                </Show>
                <div class="import-form-actions">
                  <button class="btn btn-secondary" onClick={() => { setShowImportForm(false); setImportPath(""); setImportSynced(false); }} title="Cancel import">Cancel</button>
                  <button class="btn btn-primary" onClick={handleImportSubmit} disabled={!importPath().trim() || importing()} title="Import file content into this script">
                    {importing() ? "Importing..." : "Import"}
                  </button>
                </div>
              </div>
            </Show>
          </Show>

          {/* Filename + detected language row */}
          <div class="filename-row">
            <div class="field-group field-group-grow">
              <label class="field-label" for="script-filename">Filename</label>
              <input
                id="script-filename"
                type="text"
                class={`field-input filename-input${isSynced() ? " synced-readonly" : ""}`}
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
                placeholder="e.g., setup.py, deploy.sh, config.yaml"
                disabled={isSynced()}
                data-tooltip="Script filename — extension determines language detection"
              />
            </div>
            <div class="language-indicator">
              <Show
                when={languageDetected()}
                fallback={
                  <div class="field-group">
                    <label class="field-label" for="script-language-manual">Script Language</label>
                    <input
                      id="script-language-manual"
                      type="text"
                      class={`field-input script-language-input${isSynced() ? " synced-readonly" : ""}`}
                      value={scriptLanguage()}
                      onInput={(e) => setScriptLanguage(e.currentTarget.value.toLowerCase())}
                      placeholder="e.g., python"
                      list="script-language-suggestions"
                      disabled={isSynced()}
                      data-tooltip="Programming language of this script (e.g. python, bash)"
                    />
                    <datalist id="script-language-suggestions">
                      <option value="python" /><option value="javascript" />
                      <option value="typescript" /><option value="bash" />
                      <option value="rust" /><option value="go" />
                      <option value="ruby" /><option value="java" />
                      <option value="c" /><option value="cpp" />
                      <option value="csharp" /><option value="swift" />
                      <option value="kotlin" /><option value="lua" />
                      <option value="php" /><option value="sql" />
                      <option value="html" /><option value="css" />
                      <option value="yaml" /><option value="json" />
                      <option value="xml" /><option value="toml" />
                      <option value="dockerfile" /><option value="markdown" />
                    </datalist>
                  </div>
                }
              >
                <span class="detected-label">Script Language</span>
                <div class="detected-badge" title="Auto-detected from filename extension">
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                    <path d="M5 8l2 2 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    <circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-width="1.2"/>
                  </svg>
                  {effectiveLanguage()}
                </div>
              </Show>
            </div>
          </div>

          {/* Content textarea */}
          <div class="field-group field-group-grow">
            <label class="field-label" for="script-content">Content</label>
            <textarea
              id="script-content"
              class={`field-textarea script-content${isSynced() ? " synced-readonly" : ""}`}
              value={content()}
              onInput={(e) => setContent(e.currentTarget.value)}
              placeholder="Paste or write your script content here..."
              spellcheck={false}
              disabled={isSynced()}
              data-tooltip="Script source code — supports {{variables}} for template substitution"
            />
          </div>

          {/* Description */}
          <div class="field-group">
            <label class="field-label" for="script-description">Description</label>
            <textarea
              id="script-description"
              class="field-textarea field-textarea-sm"
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              placeholder="Describe what this script does..."
              data-tooltip="Short description for search and preview"
            />
          </div>

          {/* Notes */}
          <div class="field-group">
            <label class="field-label" for="script-notes">Notes</label>
            <textarea
              id="script-notes"
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
                    <button class={`chip chip-tag${selectedTagIds().includes(tag.id) ? " selected" : ""}`} onClick={() => toggleTag(tag.id)} title={selectedTagIds().includes(tag.id) ? "Remove tag: " + tag.name : "Add tag: " + tag.name}>{tag.name}</button>
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
                    <button class={`chip chip-category${selectedCategoryIds().includes(cat.id) ? " selected" : ""}`} onClick={() => toggleCategory(cat.id)} title={selectedCategoryIds().includes(cat.id) ? "Remove category: " + cat.name : "Add category: " + cat.name}>{cat.name}</button>
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
                    <button class={`chip chip-collection${selectedCollectionIds().includes(col.id) ? " selected" : ""}`} onClick={() => toggleCollection(col.id)} title={selectedCollectionIds().includes(col.id) ? "Remove from collection: " + col.name : "Add to collection: " + col.name}>{col.name}</button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Version history */}
          <VersionPanel
            entityId={props.detail!.script.id}
            entityType="script"
            currentVersion={props.detail!.script.current_version}
            onRestore={props.onVersionRestore}
          />

          {/* Hidden file input for quick import */}
          <input ref={fileInputRef} type="file" class="file-import-hidden" onChange={handleFileImport} />

          {/* Editor toolbar */}
          <div class="editor-toolbar">
            <button class="btn btn-primary" onClick={handleSave} disabled={!isDirty() || saving()} title="Save changes (Ctrl+S)">
              {saving() ? "Saving..." : "Save"}
            </button>
            <button class="btn btn-secondary" onClick={props.onCopy} title="Copy content to clipboard (Ctrl+Shift+C)">Copy</button>
            <button class="btn btn-secondary" onClick={props.onDuplicate} title="Create a copy of this script (Ctrl+D)">Duplicate</button>
            <CopyToUserDropdown
              entityType="script"
              entityId={props.detail!.script.id}
              entityTitle={props.detail!.script.title}
            />
            <button
              class="btn btn-secondary"
              onClick={handleToggleArchive}
              attr:data-tooltip={props.detail!.script.is_archived ? "Unarchive this script" : "Archive this script"}
            >
              {props.detail!.script.is_archived ? "Unarchive" : "Archive"}
            </button>
            <button class="btn btn-danger" onClick={props.onDelete} title="Permanently delete this script">Delete</button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default ScriptEditor;
