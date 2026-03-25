/**
 * Toast component: transient notification overlay displayed in the top-right
 * corner of the application window.
 *
 * Each toast auto-dismisses after 4 seconds. Toasts display a title, message,
 * and colored left border based on type (success=green, error=red, info=cyan).
 * Pixel-perfect port of the Svelte Toast.
 */

import { Component, Show, For } from "solid-js";
import { state, actions } from "../stores/app";
import "./Toast.css";

// Re-export for backward compatibility with existing code that imports showToast
export function showToast(type: "success" | "error" | "info", title: string, message: string): void {
  actions.addToast(type, title, message);
}

const Toast: Component = () => {
  return (
    <Show when={state.toasts.length > 0}>
      <div class="toast-container" role="status" aria-live="polite">
        <For each={state.toasts}>
          {(toast) => (
            <div class={`toast toast-${toast.type}`}>
              <div class="toast-body">
                <span class="toast-title">{toast.title}</span>
                <span class="toast-message">{toast.message}</span>
              </div>
              <button
                class="toast-close"
                onClick={() => actions.dismissToast(toast.id)}
                aria-label="Dismiss notification"
                data-tooltip="Dismiss"
              >
                <svg width="12" height="12" viewBox="0 0 12 12">
                  <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.2"/>
                </svg>
              </button>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
};

export default Toast;
