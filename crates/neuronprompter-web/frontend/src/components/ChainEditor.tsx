import { logError } from "../utils/errors";
import { Component, Show, For, createSignal, createMemo, createEffect, on, onMount, onCleanup } from "solid-js";
import { api, writeToSystemClipboard, extractTemplateVariables } from "../api/client";
import { state, actions } from "../stores/app";
import type { ResolvedChainStep, ChainStepInput, Prompt, Script, ChainWithSteps } from "../api/types";
import { arraysEqual } from "../utils";
import CopyToUserDropdown from "./ui/CopyToUserDropdown";
import "./ChainEditor.css";

/** Union type for step search candidates. */
type StepCandidate =
  | { kind: "prompt"; item: Prompt }
  | { kind: "script"; item: Script };

interface ChainSnapshot {
  title: string;
  description: string;
  notes: string;
  language: string;
  separator: string;
  stepKeys: string[];
  tagIds: number[];
  categoryIds: number[];
  collectionIds: number[];
}

/** Derive a unique key for a resolved step (type + id). */
function stepKey(s: ResolvedChainStep): string {
  if (s.step.script_id !== null && s.step.script_id !== undefined) return `script:${s.step.script_id}`;
  return `prompt:${s.step.prompt_id}`;
}

/**
 * Compares two arrays for order-sensitive equality.
 * Returns true if both arrays have the same length and identical elements
 * at each index position.
 */
function arraysOrderEqual<T>(a: T[], b: T[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((v, i) => v === b[i]);
}

const SEPARATOR_PRESETS = [
  { label: "\\n\\n", value: "\n\n" },
  { label: "---", value: "\n---\n" },
  { label: "===", value: "\n===\n" },
  { label: "Custom", value: "__custom__" },
];

/**
 * ChainEditor: editor component for prompt/script chains. Displays and edits
 * the chain's metadata (title, description, notes, language, separator) and
 * its ordered list of steps. Supports drag-and-drop reordering, step search,
 * concatenated preview, taxonomy (tags/categories/collections), and dirty tracking.
 */
const ChainEditor: Component = () => {
  // ---- Form signals ----
  const [title, setTitle] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [language, setLanguage] = createSignal("");
  const [separator, setSeparator] = createSignal("\n\n");
  const [customSeparator, setCustomSeparator] = createSignal("");
  const [steps, setSteps] = createSignal<ResolvedChainStep[]>([]);
  const [selectedTagIds, setSelectedTagIds] = createSignal<number[]>([]);
  const [selectedCategoryIds, setSelectedCategoryIds] = createSignal<number[]>([]);
  const [selectedCollectionIds, setSelectedCollectionIds] = createSignal<number[]>([]);
  const [saving, setSaving] = createSignal(false);

  // ---- Step search ----
  const [stepSearchQuery, setStepSearchQuery] = createSignal("");
  const [stepSearchOpen, setStepSearchOpen] = createSignal(false);

  // ---- Preview ----
  const [previewOpen, setPreviewOpen] = createSignal(false);

  // ---- Expand state per step ----
  const [expandedSteps, setExpandedSteps] = createSignal<Set<number>>(new Set());

  // ---- Drag state ----
  const [dragIndex, setDragIndex] = createSignal<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = createSignal<number | null>(null);

  // ---- Snapshot for dirty tracking ----
  const [snapshot, setSnapshot] = createSignal<ChainSnapshot>({
    title: "",
    description: "",
    notes: "",
    language: "",
    separator: "\n\n",
    stepKeys: [],
    tagIds: [],
    categoryIds: [],
    collectionIds: [],
  });

  const detail = () => state.activeChainDetail;

  // Sync form fields when the active chain changes.
  // Track primitive values (id + updated_at) to detect switches reliably,
  // because SolidJS store proxies keep the same reference even after reconcile().
  createEffect(
    on(
      () => { const d = detail(); return d ? `${d.chain.id}:${d.chain.updated_at}` : null; },
      () => {
      const d = detail();
      if (d) {
        const c = d.chain;
        setTitle(c.title);
        setDescription(c.description ?? "");
        setNotes(c.notes ?? "");
        setLanguage(c.language ?? "");

        const sep = c.separator;
        const isPreset = SEPARATOR_PRESETS.some((p) => p.value === sep);
        if (isPreset) {
          setSeparator(sep);
          setCustomSeparator("");
        } else {
          setSeparator("__custom__");
          setCustomSeparator(sep);
        }

        setSteps([...d.steps]);
        setSelectedTagIds(d.tags.map((t) => t.id));
        setSelectedCategoryIds(d.categories.map((c) => c.id));
        setSelectedCollectionIds(d.collections.map((c) => c.id));
        setExpandedSteps(new Set<number>());

        const snap: ChainSnapshot = {
          title: c.title,
          description: c.description ?? "",
          notes: c.notes ?? "",
          language: c.language ?? "",
          separator: c.separator,
          stepKeys: d.steps.map((s) => stepKey(s)),
          tagIds: d.tags.map((t) => t.id),
          categoryIds: d.categories.map((c) => c.id),
          collectionIds: d.collections.map((c) => c.id),
        };
        setSnapshot(snap);
      }
    }),
  );

  const effectiveSeparator = createMemo(() => {
    return separator() === "__custom__" ? customSeparator() : separator();
  });

  const isDirty = createMemo(() => {
    const snap = snapshot();
    return (
      title() !== snap.title ||
      description() !== snap.description ||
      notes() !== snap.notes ||
      language() !== snap.language ||
      effectiveSeparator() !== snap.separator ||
      !arraysOrderEqual(
        steps().map((s) => stepKey(s)),
        snap.stepKeys,
      ) ||
      !arraysEqual(selectedTagIds(), snap.tagIds) ||
      !arraysEqual(selectedCategoryIds(), snap.categoryIds) ||
      !arraysEqual(selectedCollectionIds(), snap.collectionIds)
    );
  });

  createEffect(() => {
    actions.setChainEditorDirty(isDirty());
  });

  // M-54: Register this editor's save handler so Ctrl+S works via the store,
  // matching the pattern used in PromptEditor and ScriptEditor.
  onMount(() => {
    actions.setSaveHandler(() => handleSave());
  });
  onCleanup(() => {
    actions.setSaveHandler(null);
  });

  // ---- Concatenated preview ----
  const concatenatedContent = createMemo(() => {
    const sep = effectiveSeparator();
    return steps()
      .map((s) => s.prompt?.content ?? s.script?.content ?? "")
      .join(sep);
  });

  // ---- Duplicate detection ----
  const duplicateStepKeys = createMemo(() => {
    const counts = new Map<string, number>();
    for (const s of steps()) {
      const k = stepKey(s);
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    const dups = new Set<string>();
    for (const [k, count] of counts) {
      if (count > 1) dups.add(k);
    }
    return dups;
  });

  // ---- Step search filtering ----
  const filteredCandidates = createMemo<StepCandidate[]>(() => {
    const q = stepSearchQuery().toLowerCase().trim();
    const promptCandidates: StepCandidate[] = (q
      ? state.prompts.filter(
          (p) => p.title.toLowerCase().includes(q),
        )
      : state.prompts
    ).slice(0, 15).map((p) => ({ kind: "prompt", item: p }));

    const scriptCandidates: StepCandidate[] = (q
      ? state.scripts.filter(
          (s) => s.title.toLowerCase().includes(q),
        )
      : state.scripts
    ).slice(0, 15).map((s) => ({ kind: "script", item: s }));

    return [...promptCandidates, ...scriptCandidates].slice(0, 20);
  });

  // ---- Handlers ----

  const handleSave = async () => {
    const d = detail();
    if (!d || saving()) return;
    setSaving(true);
    try {
      const stepInputs: ChainStepInput[] = steps().map((s) => {
        if (s.step.script_id !== null && s.step.script_id !== undefined) {
          return { step_type: "script" as const, item_id: s.step.script_id };
        }
        return { step_type: "prompt" as const, item_id: s.step.prompt_id! };
      });
      await api.updateChain({
        chain_id: d.chain.id,
        title: title(),
        description: description() || null,
        notes: notes() || null,
        language: language() || null,
        separator: effectiveSeparator(),
        steps: stepInputs,
        tag_ids: selectedTagIds(),
        category_ids: selectedCategoryIds(),
        collection_ids: selectedCollectionIds(),
      });
      setSnapshot({
        title: title(),
        description: description(),
        notes: notes(),
        language: language(),
        separator: effectiveSeparator(),
        stepKeys: steps().map((s) => stepKey(s)),
        tagIds: [...selectedTagIds()],
        categoryIds: [...selectedCategoryIds()],
        collectionIds: [...selectedCollectionIds()],
      });
      actions.addToast("success", "Chain saved", "Chain updated successfully.");
      // Refresh the detail
      const refreshed = await api.getChain(d.chain.id);
      actions.setActiveChainDetail(refreshed);
    } catch (err) {
      logError("ChainEditor.save", err);
      actions.addToast("error", "Save failed", String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleCopyChain = async () => {
    const d = detail();
    if (!d) return;
    try {
      const { content } = await api.getChainContent(d.chain.id);
      const vars = extractTemplateVariables(content);
      if (vars.length > 0) {
        actions.openTemplateDialog(vars, content, d.chain.title);
        return;
      }
      const clipOk = writeToSystemClipboard(content);
      if (clipOk) {
        actions.addToast("success", "Copied", "Chain content copied to clipboard.");
      } else {
        actions.addToast("error", "Clipboard Error", "Failed to copy to system clipboard");
      }
      api.copyToClipboard(content, d.chain.title).catch(() => {});
    } catch (err) {
      logError("ChainEditor.copy", err);
      actions.addToast("error", "Copy failed", String(err));
    }
  };

  const handleDuplicate = async () => {
    const d = detail();
    if (!d) return;
    try {
      const newChain = await api.duplicateChain(d.chain.id);
      actions.addToast("success", "Duplicated", `Created "${newChain.title}".`);
      // Refresh chain list
      if (state.activeUser) {
        const chains = (await api.listChains({ user_id: state.activeUser.id })).items;
        actions.setChains(chains);
        actions.setActiveChainId(newChain.id);
        const newDetail = await api.getChain(newChain.id);
        actions.setActiveChainDetail(newDetail);
      }
    } catch (err) {
      logError("ChainEditor.duplicate", err);
      actions.addToast("error", "Duplicate failed", String(err));
    }
  };

  const handleToggleArchive = async () => {
    const d = detail();
    if (!d) return;
    const newValue = !d.chain.is_archived;
    try {
      await api.toggleChainArchive(d.chain.id, newValue);
      actions.addToast("success", newValue ? "Archived" : "Unarchived", `Chain "${d.chain.title}" ${newValue ? "archived" : "unarchived"}.`);
      // Refresh
      const refreshed = await api.getChain(d.chain.id);
      actions.setActiveChainDetail(refreshed);
      if (state.activeUser) {
        const chains = (await api.listChains({ user_id: state.activeUser.id })).items;
        actions.setChains(chains);
      }
    } catch (err) {
      logError("ChainEditor.toggleArchive", err);
      actions.addToast("error", "Archive toggle failed", String(err));
    }
  };

  const handleDelete = async () => {
    const d = detail();
    if (!d) return;
    if (!window.confirm("Delete this chain?")) return;
    try {
      await api.deleteChain(d.chain.id);
      actions.addToast("success", "Deleted", `Chain "${d.chain.title}" deleted.`);
      actions.setActiveChainId(null);
      actions.setActiveChainDetail(null);
      if (state.activeUser) {
        const chains = (await api.listChains({ user_id: state.activeUser.id })).items;
        actions.setChains(chains);
      }
    } catch (err) {
      logError("ChainEditor.delete", err);
      actions.addToast("error", "Delete failed", String(err));
    }
  };

  // ---- Step manipulation ----

  const addStep = (candidate: StepCandidate) => {
    const current = steps();
    const chainId = detail()?.chain.id ?? 0;
    let newStep: ResolvedChainStep;
    if (candidate.kind === "prompt") {
      newStep = {
        step: {
          id: 0,
          chain_id: chainId,
          step_type: "prompt",
          prompt_id: candidate.item.id,
          script_id: null,
          position: current.length,
        },
        prompt: candidate.item,
        script: null,
      };
    } else {
      newStep = {
        step: {
          id: 0,
          chain_id: chainId,
          step_type: "script",
          prompt_id: null,
          script_id: candidate.item.id,
          position: current.length,
        },
        prompt: null,
        script: candidate.item,
      };
    }
    setSteps([...current, newStep]);
    setStepSearchQuery("");
    setStepSearchOpen(false);
  };

  const removeStep = (index: number) => {
    setSteps(steps().filter((_, i) => i !== index));
  };

  const moveStep = (from: number, to: number) => {
    if (from === to) return;
    const arr = [...steps()];
    const [item] = arr.splice(from, 1);
    arr.splice(to, 0, item!);
    setSteps(arr);
  };

  const toggleExpand = (index: number) => {
    const set = new Set(expandedSteps());
    if (set.has(index)) {
      set.delete(index);
    } else {
      set.add(index);
    }
    setExpandedSteps(set);
  };

  // ---- Drag and drop ----

  const handleDragStart = (index: number, e: DragEvent) => {
    setDragIndex(index);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(index));
    }
  };

  const handleDragOver = (index: number, e: DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "move";
    }
    setDragOverIndex(index);
  };

  const handleDragLeave = () => {
    setDragOverIndex(null);
  };

  const handleDrop = (index: number, e: DragEvent) => {
    e.preventDefault();
    const from = dragIndex();
    if (from !== null && from !== index) {
      moveStep(from, index);
    }
    setDragIndex(null);
    setDragOverIndex(null);
  };

  const handleDragEnd = () => {
    setDragIndex(null);
    setDragOverIndex(null);
  };

  // ---- Taxonomy toggles ----

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

  // ---- Copy preview ----

  const handleCopyPreview = () => {
    const ok = writeToSystemClipboard(concatenatedContent());
    if (ok) {
      actions.addToast("success", "Copied", "Preview content copied to clipboard.");
    } else {
      actions.addToast("error", "Copy failed", "Could not copy to clipboard.");
    }
  };

  return (
    <div class="chain-editor">
      <Show
        when={detail()}
        fallback={
          <div class="chain-editor-empty">
            <p class="chain-empty-title">No chain selected</p>
            <p class="chain-empty-hint">Select a chain from the list or create a new one</p>
          </div>
        }
        keyed
      >
        {(d: ChainWithSteps) => (
        <div class="chain-editor-content">
          {/* Title + Language row */}
          <div class="metadata-row">
            <div class="field-group field-group-grow-row">
              <label class="field-label" for="chain-title">Title</label>
              <input
                id="chain-title"
                type="text"
                class="field-input"
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
                placeholder="Chain title"
                data-tooltip="Chain title -- displayed in the list"
              />
            </div>
            <div class="field-group field-group-lang">
              <label class="field-label" for="chain-language">Language</label>
              <input
                id="chain-language"
                type="text"
                class="field-input"
                value={language()}
                onInput={(e) => setLanguage(e.currentTarget.value)}
                placeholder="e.g., English"
                data-tooltip="Natural language of this chain (e.g. en, de)"
              />
            </div>
          </div>

          {/* Description */}
          <div class="field-group">
            <label class="field-label" for="chain-description">Description</label>
            <textarea
              id="chain-description"
              class="field-textarea field-textarea-sm"
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              placeholder="Describe what this chain does..."
              data-tooltip="Short description for search and preview"
            />
          </div>

          {/* Notes */}
          <div class="field-group">
            <label class="field-label" for="chain-notes">Notes</label>
            <textarea
              id="chain-notes"
              class="field-textarea field-textarea-sm"
              value={notes()}
              onInput={(e) => setNotes(e.currentTarget.value)}
              placeholder="Personal notes..."
              data-tooltip="Private notes -- not included when copying"
            />
          </div>

          {/* Separator configuration */}
          <div class="field-group">
            <span class="field-label">Separator</span>
            <div class="separator-chips">
              <For each={SEPARATOR_PRESETS}>
                {(preset) => (
                  <button
                    class={`chip chip-separator${
                      separator() === preset.value ||
                      (preset.value === "__custom__" && separator() === "__custom__")
                        ? " selected"
                        : ""
                    }`}
                    onClick={() => setSeparator(preset.value)}
                    attr:data-tooltip={"Join steps with: " + preset.label}
                  >
                    {preset.label}
                  </button>
                )}
              </For>
            </div>
            <Show when={separator() === "__custom__"}>
              <input
                type="text"
                class="field-input chain-custom-separator"
                value={customSeparator()}
                onInput={(e) => setCustomSeparator(e.currentTarget.value)}
                placeholder="Custom separator text..."
                data-tooltip="Custom separator text inserted between chain steps"
              />
            </Show>
          </div>

          {/* Steps section */}
          <div class="field-group">
            <div class="chain-steps-header">
              <span class="field-label">
                Steps
                <span class="chain-count-badge">{steps().length}</span>
              </span>
              <button
                class="btn btn-secondary btn-sm"
                onClick={() => setStepSearchOpen(!stepSearchOpen())}
                data-tooltip="Add a prompt or script step to this chain"
              >
                Add Step
              </button>
            </div>

            {/* Add step search */}
            <Show when={stepSearchOpen()}>
              <div class="chain-step-search">
                <input
                  type="text"
                  class="field-input"
                  value={stepSearchQuery()}
                  onInput={(e) => setStepSearchQuery(e.currentTarget.value)}
                  placeholder="Search prompts & scripts to add..."
                  autofocus
                  data-tooltip="Search for prompts and scripts to add as steps"
                />
                <Show when={filteredCandidates().length > 0}>
                  <div class="chain-step-search-results">
                    <For each={filteredCandidates()}>
                      {(candidate) => (
                        <button
                          class="chain-step-search-item"
                          onClick={() => addStep(candidate)}
                        >
                          <span class="chain-step-search-title">
                            <span class={`chain-step-type-badge ${candidate.kind}-badge`}>
                              {candidate.kind === "prompt" ? "Prompt" : "Script"}
                            </span>
                            {candidate.item.title}
                          </span>
                          <span class="chain-step-search-preview">
                            {candidate.item.content.slice(0, 80)}
                            {candidate.item.content.length > 80 ? "..." : ""}
                          </span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>

            {/* Step cards */}
            <div class="chain-steps-list">
              <For each={steps()}>
                {(step, i) => (
                  <div
                    class={`chain-step-card${dragOverIndex() === i() ? " drag-over" : ""}${dragIndex() === i() ? " dragging" : ""}`}
                    draggable={true}
                    onDragStart={(e) => handleDragStart(i(), e)}
                    onDragOver={(e) => handleDragOver(i(), e)}
                    onDragLeave={handleDragLeave}
                    onDrop={(e) => handleDrop(i(), e)}
                    onDragEnd={handleDragEnd}
                  >
                    <div class="chain-step-card-header">
                      <span class="chain-step-position">{i() + 1}</span>
                      <span class={`chain-step-type-badge ${step.script ? "script" : "prompt"}-badge`}>
                        {step.script ? "Script" : "Prompt"}
                      </span>
                      <span class="chain-step-title">
                        {step.prompt?.title ?? step.script?.title ?? "Unknown"}
                      </span>
                      <Show when={step.script?.script_language}>
                        <span class="chain-step-lang-badge">{step.script!.script_language}</span>
                      </Show>
                      <Show when={duplicateStepKeys().has(stepKey(step))}>
                        <span class="chain-step-duplicate-badge">Duplicate</span>
                      </Show>
                      <div class="chain-step-actions">
                        <button
                          class="chain-step-btn"
                          onClick={() => toggleExpand(i())}
                          attr:data-tooltip={expandedSteps().has(i()) ? "Collapse" : "Expand"}
                        >
                          {expandedSteps().has(i()) ? "\u25B2" : "\u25BC"}
                        </button>
                        <button
                          class="chain-step-btn"
                          onClick={() => moveStep(i(), Math.max(0, i() - 1))}
                          disabled={i() === 0}
                          data-tooltip="Move up"
                        >
                          {"\u2191"}
                        </button>
                        <button
                          class="chain-step-btn"
                          onClick={() => moveStep(i(), Math.min(steps().length - 1, i() + 1))}
                          disabled={i() === steps().length - 1}
                          data-tooltip="Move down"
                        >
                          {"\u2193"}
                        </button>
                        <button
                          class="chain-step-btn chain-step-btn-remove"
                          onClick={() => removeStep(i())}
                          data-tooltip="Remove step"
                        >
                          {"\u2715"}
                        </button>
                      </div>
                    </div>
                    {(() => {
                      const content = step.prompt?.content ?? step.script?.content ?? "";
                      return (
                        <>
                          <Show when={!expandedSteps().has(i())}>
                            <p class="chain-step-preview">
                              {content.slice(0, 120)}
                              {content.length > 120 ? "..." : ""}
                            </p>
                          </Show>
                          <Show when={expandedSteps().has(i())}>
                            <pre class="chain-step-full-content">{content}</pre>
                          </Show>
                        </>
                      );
                    })()}
                  </div>
                )}
              </For>
            </div>
          </div>

          {/* Concatenated preview */}
          <div class="field-group">
            <div class="chain-preview-header">
              <button
                class="chain-preview-toggle"
                onClick={() => setPreviewOpen(!previewOpen())}
                data-tooltip="Show/hide the composed output of all steps"
              >
                <span class="field-label">
                  Concatenated Preview
                  <span class="chain-count-badge">{concatenatedContent().length} chars</span>
                </span>
                <span>{previewOpen() ? "\u25B2" : "\u25BC"}</span>
              </button>
              <button class="btn btn-secondary btn-sm" onClick={handleCopyPreview} title="Copy concatenated chain output to clipboard">
                Copy
              </button>
            </div>
            <Show when={previewOpen()}>
              <pre class="chain-preview-content">{concatenatedContent()}</pre>
            </Show>
          </div>

          {/* Tags */}
          <Show when={state.tags.length > 0}>
            <div class="field-group">
              <span class="field-label">Tags</span>
              <div class="chip-container">
                <For each={state.tags}>
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
          <Show when={state.categories.length > 0}>
            <div class="field-group">
              <span class="field-label">Categories</span>
              <div class="chip-container">
                <For each={state.categories}>
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
          <Show when={state.collections.length > 0}>
            <div class="field-group">
              <span class="field-label">Collections</span>
              <div class="chip-container">
                <For each={state.collections}>
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

          {/* Editor toolbar -- M-51: uses keyed Show callback value `d` instead
              of detail()! non-null assertions to avoid potential null dereference. */}
          <div class="editor-toolbar">
            <button class="btn btn-primary" onClick={handleSave} disabled={!isDirty() || saving()} title="Save changes (Ctrl+S)">
              {saving() ? "Saving..." : "Save"}
            </button>
            <button class="btn btn-secondary" onClick={handleCopyChain} title="Copy composed chain content to clipboard (Ctrl+Shift+C)">Copy Chain</button>
            <button class="btn btn-secondary" onClick={handleDuplicate} title="Create a copy of this chain (Ctrl+D)">Duplicate</button>
            <CopyToUserDropdown
              entityType="chain"
              entityId={d.chain.id}
              entityTitle={d.chain.title}
            />
            <button
              class="btn btn-secondary"
              onClick={handleToggleArchive}
              attr:data-tooltip={d.chain.is_archived ? "Unarchive this chain" : "Archive this chain"}
            >
              {d.chain.is_archived ? "Unarchive" : "Archive"}
            </button>
            <button class="btn btn-danger" onClick={handleDelete} title="Permanently delete this chain and its steps">Delete</button>
          </div>
        </div>
        )}
      </Show>
    </div>
  );
};

export default ChainEditor;
