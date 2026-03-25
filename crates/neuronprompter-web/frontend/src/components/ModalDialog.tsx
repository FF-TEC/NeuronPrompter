/**
 * ModalDialog component: reusable centered dialog for confirmations,
 * user creation, import operations, and other modal interactions.
 *
 * Renders a glass-morphism surface with a backdrop blur overlay.
 * Closes on Escape key press or backdrop click.
 * Pixel-perfect port of the Svelte ModalDialog.
 */

import { Component, Show, JSX, createEffect, onCleanup } from "solid-js";

interface ModalDialogProps {
  /** Title displayed in the modal header. */
  title: string;
  /** Whether the modal is currently visible. */
  open: boolean;
  /** Callback fired when the modal should close. */
  onClose: () => void;
  /** Content rendered inside the modal body. */
  children: JSX.Element;
}

/** CSS selector matching all natively focusable elements. */
const FOCUSABLE_SELECTOR = 'a[href], button:not([disabled]), textarea, input:not([disabled]), select, [tabindex]:not([tabindex="-1"])';

const ModalDialog: Component<ModalDialogProps> = (props) => {
  let surfaceRef: HTMLDivElement | undefined;

  /**
   * L-54: Reference to the backdrop div. Used in handleBackdropClick to compare
   * the event target by identity rather than relying on className checks,
   * which is more robust against CSS class refactoring.
   */
  let backdropRef: HTMLDivElement | undefined;

  /** Handles Escape to close and Tab for focus trapping within the modal surface. */
  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      props.onClose();
      return;
    }

    // Focus trap: keep Tab cycling within the modal
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
   * L-54: Closes the modal when the user clicks directly on the backdrop element.
   * Compares event.target against the stored ref instead of checking CSS class names.
   */
  function handleBackdropClick(e: MouseEvent): void {
    if (e.target === backdropRef) {
      props.onClose();
    }
  }

  /**
   * M-47: The keydown listener is registered and unregistered reactively
   * based on props.open. This avoids registering listeners on mount when
   * the dialog may not be open, and automatically cleans up via SolidJS's
   * effect disposal when the dialog closes.
   */
  createEffect(() => {
    if (props.open) {
      window.addEventListener("keydown", handleKeydown);
      onCleanup(() => window.removeEventListener("keydown", handleKeydown));
    }
  });

  return (
    <Show when={props.open}>
      <div
        class="modal-backdrop"
        ref={backdropRef}
        onClick={handleBackdropClick}
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        tabindex="-1"
      >
        <div class="modal-surface" ref={surfaceRef}>
          <div class="modal-header">
            <h2 class="modal-title">{props.title}</h2>
            <button class="modal-close" onClick={props.onClose} aria-label="Close dialog">
              <svg width="16" height="16" viewBox="0 0 16 16">
                <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
            </button>
          </div>
          <div class="modal-body">
            {props.children}
          </div>
        </div>
      </div>
    </Show>
  );
};

export default ModalDialog;
