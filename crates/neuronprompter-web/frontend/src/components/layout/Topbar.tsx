/**
 * Topbar component: 56px tab-navigation bar at the top of the window.
 *
 * Contains the NeuronPrompter logo, tab buttons (left: Organize, Prompts,
 * Scripts, Chains, Clipboard; right: Users, Models, Settings), and window
 * controls. The empty spacer area between left and right tabs serves as the
 * window drag region for the frameless native window (via wry IPC).
 */

import { Component } from "solid-js";
import { state, type AppTab } from "../../stores/app";

/**
 * Initiates a native window drag operation via wry IPC.
 * Called on mousedown within designated drag regions (spacer, logo, topbar background).
 * The Rust backend handles the "drag" message by calling `window.drag_window()`.
 */
function startWindowDrag(): void {
  window.ipc?.postMessage("drag");
}

const Topbar: Component<{
  activeTab: AppTab;
  onTabChange: (tab: AppTab) => void;
}> = (props) => {
  return (
    <header class="topbar" onMouseDown={startWindowDrag}>
      {/* Logo area: acts as drag region for the frameless window */}
      <div class="topbar-logo" onMouseDown={startWindowDrag}>
        <h1 class="logo">
          <span class="logo-neuron">Neuron</span>
          <span class="logo-prompter">Prompter</span>
        </h1>
      </div>

      {/* Left tabs: stop mousedown propagation so clicks on tabs do not trigger window drag */}
      <div class="tab-bar" role="tablist" aria-label="Navigation" onMouseDown={(e) => e.stopPropagation()}>
        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "organize"}
          aria-controls="tabpanel-organize"
          onClick={() => props.onTabChange("organize")}
          data-tooltip="Manage tags, categories, and collections (Ctrl+1)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <path d="M2 4.5h3l1-1.5h4l1 1.5h3v9a1 1 0 01-1 1H3a1 1 0 01-1-1v-9z" stroke="currentColor" stroke-width="1.2"/>
            <path d="M5 8.5h6M5 11h4" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Organize</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "prompts"}
          aria-controls="tabpanel-prompts"
          onClick={() => props.onTabChange("prompts")}
          data-tooltip="Browse and edit prompts (Ctrl+2)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <path d="M4 2h8a1 1 0 011 1v10a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1z" stroke="currentColor" stroke-width="1.2"/>
            <path d="M5.5 5h5M5.5 7.5h5M5.5 10h3" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Prompts</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "scripts"}
          aria-controls="tabpanel-scripts"
          onClick={() => props.onTabChange("scripts")}
          data-tooltip="Browse and edit scripts (Ctrl+3)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <path d="M5.5 4L3 8l2.5 4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="M10.5 4L13 8l-2.5 4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="M9 2.5L7 13.5" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Scripts</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "chains"}
          aria-controls="tabpanel-chains"
          onClick={() => props.onTabChange("chains")}
          data-tooltip="Compose prompt and script chains (Ctrl+4)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <path d="M7 9l2-2" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <path d="M5.5 7.5a2.5 2.5 0 010-3.5l1-1a2.5 2.5 0 013.5 3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <path d="M10.5 8.5a2.5 2.5 0 010 3.5l-1 1a2.5 2.5 0 01-3.5-3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Chains</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "clipboard"}
          aria-controls="tabpanel-clipboard"
          onClick={() => props.onTabChange("clipboard")}
          data-tooltip="Advanced search and copy workspace (Ctrl+5)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <rect x="4" y="1" width="8" height="3" rx="1" stroke="currentColor" stroke-width="1.1"/>
            <path d="M3 4h10v10a1 1 0 01-1 1H4a1 1 0 01-1-1V4z" stroke="currentColor" stroke-width="1.2"/>
            <path d="M6 8h4M6 10.5h4" stroke="currentColor" stroke-width="1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Clipboard</span>
        </button>
      </div>

      {/* Spacer: primary drag region for window movement */}
      <div class="spacer" onMouseDown={startWindowDrag}></div>

      {/* Right tabs: stop mousedown propagation so clicks on tabs do not trigger window drag */}
      <div class="tab-bar tab-bar-right" role="tablist" aria-label="Settings" onMouseDown={(e) => e.stopPropagation()}>
        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "users"}
          aria-controls="tabpanel-users"
          onClick={() => props.onTabChange("users")}
          attr:data-tooltip={state.activeUser ? `Active: ${state.activeUser.display_name}` : "Users"}
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <circle cx="5.5" cy="5" r="2" stroke="currentColor" stroke-width="1.1"/>
            <path d="M1.5 13c0-2.2 1.8-4 4-4s4 1.8 4 4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <circle cx="11" cy="5.5" r="1.8" stroke="currentColor" stroke-width="1.1"/>
            <path d="M11 8.5c1.7 0 3.2 1.3 3.5 3" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">{state.activeUser?.display_name ?? "Users"}</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "models"}
          aria-controls="tabpanel-models"
          onClick={() => props.onTabChange("models")}
          data-tooltip="Configure Ollama model connection"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <rect x="4" y="4" width="8" height="8" rx="1.5" stroke="currentColor" stroke-width="1.2"/>
            <circle cx="8" cy="8" r="1.5" fill="currentColor"/>
            <path d="M4 7H2M4 9H2M12 7h2M12 9h2M7 4V2M9 4V2M7 12v2M9 12v2" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Models</span>
        </button>

        <button
          class="tab-btn"
          role="tab"
          aria-selected={props.activeTab === "settings"}
          aria-controls="tabpanel-settings"
          onClick={() => props.onTabChange("settings")}
          data-tooltip="Settings (Ctrl+,)"
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.2"/>
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
          <span class="tab-label">Settings</span>
        </button>
      </div>

      {/* Window controls (minimize, maximize, close): stop mousedown propagation to prevent drag */}
      <div class="window-controls" onMouseDown={(e) => e.stopPropagation()}>
        <button
          class="window-btn window-btn-minimize"
          onClick={() => window.ipc?.postMessage("minimize")}
          data-tooltip="Minimize"
          aria-label="Minimize"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M2 6h8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
        </button>
        <button
          class="window-btn window-btn-maximize"
          onClick={() => window.ipc?.postMessage("maximize")}
          data-tooltip="Maximize"
          aria-label="Maximize"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <rect x="2" y="2" width="8" height="8" rx="1" stroke="currentColor" stroke-width="1.2"/>
          </svg>
        </button>
        <button
          class="window-btn window-btn-close"
          onClick={() => window.ipc?.postMessage("close")}
          data-tooltip="Close"
          aria-label="Close"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
        </button>
      </div>
    </header>
  );
};

export default Topbar;
