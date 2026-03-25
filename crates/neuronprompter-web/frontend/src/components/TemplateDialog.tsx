/**
 * TemplateDialog component: modal dialog for filling template variable placeholders.
 *
 * When a prompt contains {{variable}} placeholders, this dialog displays an
 * input field for each detected variable. The user fills in values, and the
 * content is copied to the clipboard with all variables substituted.
 * Pixel-perfect port of the Svelte TemplateDialog.
 */

import { Component, Show, For, createSignal, createEffect, onCleanup, untrack } from "solid-js";
import { api, writeToSystemClipboard } from "../api/client";
import { showToast } from "./Toast";

interface TemplateDialogProps {
  /** Whether the dialog is visible. */
  open: boolean;
  /** Detected template variable names from the prompt content. */
  variables: string[];
  /** The raw prompt content containing {{variable}} placeholders. */
  content: string;
  /** The prompt title for clipboard history tracking. */
  promptTitle: string;
  /** Callback fired when the dialog should close. */
  onClose: () => void;
}

/**
 * CSS selector matching all natively focusable elements within a dialog.
 * Mirrors the same selector used by ModalDialog for consistency across
 * all modal components in the application.
 */
const FOCUSABLE_SELECTOR = 'a[href], button:not([disabled]), textarea, input:not([disabled]), select, [tabindex]:not([tabindex="-1"])';

const TemplateDialog: Component<TemplateDialogProps> = (props) => {
  const [values, setValues] = createSignal<Record<string, string>>({});

  /**
   * Reference to the dialog surface element. Used by the focus trap logic
   * to query all focusable children and cycle Tab focus within the dialog
   * boundary, preventing keyboard focus from escaping to the page behind.
   */
  let surfaceRef: HTMLDivElement | undefined;

  /**
   * Reference to the backdrop element. Used by handleBackdropClick to compare
   * the click target by identity rather than relying on CSS class name checks,
   * which is more robust against CSS class refactoring.
   */
  let backdropRef: HTMLDivElement | undefined;

  /** Resets variable values when the dialog opens or the variable list changes.
   *  Reads values() inside untrack() to preserve previously entered input
   *  without creating a tracking dependency that would cause a circular
   *  write-read loop (setValues -> effect re-runs -> setValues again). */
  createEffect(() => {
    if (props.open) {
      const fresh: Record<string, string> = {};
      const current = untrack(() => values());
      for (const v of props.variables) {
        fresh[v] = current[v] ?? "";
      }
      setValues(fresh);
    }
  });

  /** Copies the content with template variables substituted. */
  async function handleSubmit(): Promise<void> {
    try {
      const substituted = await api.copyWithSubstitution(props.content, props.promptTitle, values());
      const clipOk = writeToSystemClipboard(substituted);
      if (clipOk) {
        showToast("success", "Copied", "Content copied with variables substituted");
      } else {
        showToast("error", "Clipboard Error", "Failed to copy to system clipboard");
      }
      props.onClose();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      showToast("error", "Error", message);
    }
  }

  /** Returns true when all variable fields have a non-empty value.
   *  Defined as a plain function rather than a createMemo so that SolidJS
   *  JSX attribute effects track the values() signal read directly. This
   *  avoids memo caching issues with store proxy array iteration. */
  const allFilled = (): boolean => {
    const current = values();
    const vars = props.variables;
    if (vars.length === 0) return false;
    return vars.every((v) => (current[v] ?? "").trim().length > 0);
  };

  /**
   * M-48: Closes the dialog when the user clicks directly on the backdrop.
   * Compares event.target against the stored ref by identity rather than
   * checking CSS class names.
   */
  function handleBackdropClick(e: MouseEvent): void {
    if (e.target === backdropRef) {
      props.onClose();
    }
  }

  /**
   * M-48: Handles keyboard interaction for the template dialog.
   * - Escape closes the dialog.
   * - Tab / Shift+Tab cycles focus within the dialog surface, preventing
   *   keyboard focus from escaping to the page behind the modal overlay.
   *   This focus trap implementation mirrors the pattern used by ModalDialog.
   */
  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      props.onClose();
      return;
    }

    // Focus trap: keep Tab cycling within the dialog surface
    if (e.key === "Tab" && surfaceRef) {
      const focusable = Array.from(surfaceRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last?.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first?.focus();
        }
      }
    }
  }

  /**
   * M-48: The keydown listener is registered and unregistered reactively
   * based on props.open. This avoids stale listeners when the dialog is
   * closed, and automatically cleans up via SolidJS effect disposal.
   */
  createEffect(() => {
    if (props.open) {
      window.addEventListener("keydown", handleKeydown);
      onCleanup(() => window.removeEventListener("keydown", handleKeydown));
    }
  });

  function updateValue(variable: string, value: string): void {
    setValues((prev) => ({ ...prev, [variable]: value }));
  }

  return (
    <Show when={props.open}>
      <div
        class="template-backdrop"
        ref={backdropRef}
        onClick={handleBackdropClick}
        role="dialog"
        aria-modal="true"
        aria-label="Template Variables"
        tabindex="-1"
      >
        <div class="template-surface" ref={surfaceRef}>
          <div class="template-header">
            <h2 class="template-title">Template Variables</h2>
            <button class="template-close" onClick={props.onClose} aria-label="Close dialog">
              <svg width="16" height="16" viewBox="0 0 16 16">
                <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
            </button>
          </div>

          <form
            class="template-body"
            onSubmit={(e) => { e.preventDefault(); handleSubmit(); }}
          >
            <p class="template-hint">
              Fill in values for the template variables found in this prompt.
            </p>
            <For each={props.variables}>
              {(variable) => (
                <div class="template-field">
                  <label class="template-label" for={`var-${variable}`}>
                    {`{{${variable}}}`}
                  </label>
                  <input
                    id={`var-${variable}`}
                    type="text"
                    class="template-input"
                    value={values()[variable] ?? ""}
                    onInput={(e) => updateValue(variable, e.currentTarget.value)}
                    placeholder={`Enter value for ${variable}`}
                  />
                </div>
              )}
            </For>

            <div class="template-actions">
              <button
                type="button"
                class="template-btn template-btn-secondary"
                onClick={props.onClose}
              >
                Cancel
              </button>
              <button
                type="submit"
                class="template-btn template-btn-primary"
                disabled={!allFilled()}
              >
                Copy with Values
              </button>
            </div>
          </form>
        </div>
      </div>
    </Show>
  );
};

export default TemplateDialog;
