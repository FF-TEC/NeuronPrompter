/**
 * StatusBar component: 36px fixed-height bar at the bottom of the window.
 *
 * Displays Ollama connection status (cyan dot), MCP server status (purple dot),
 * the active user name, a prompt count indicator, and a log panel toggle.
 * Pixel-perfect port of the Svelte Statusbar.
 */

import { Component, Show } from "solid-js";
import { state } from "../../stores/app";

const StatusBar: Component<{
  ollamaConnected: boolean;
  mcpRegistered: boolean;
  promptCount: number;
  scriptCount: number;
  chainCount: number;
  logPanelOpen: boolean;
  logMessageCount: number;
  onToggleLogPanel: () => void;
}> = (props) => {
  return (
    <footer class="statusbar">
      <div class="statusbar-left">
        <span class="status-indicator" title={props.ollamaConnected ? "Ollama connected" : "Ollama disconnected"}>
          <span class={`status-dot${props.ollamaConnected ? " connected-cyan" : ""}`}></span>
          <span class="status-label">Ollama</span>
        </span>
        <span class="status-separator"></span>
        <span class="status-indicator" title={props.mcpRegistered ? "MCP server running" : "MCP server stopped"}>
          <span class={`status-dot${props.mcpRegistered ? " connected-purple" : ""}`}></span>
          <span class="status-label">MCP</span>
        </span>
      </div>

      <div class="statusbar-right">
        <span class="status-info">
          {props.promptCount} {props.promptCount === 1 ? "prompt" : "prompts"} | {props.scriptCount} {props.scriptCount === 1 ? "script" : "scripts"} | {props.chainCount} {props.chainCount === 1 ? "chain" : "chains"}
        </span>
        <Show when={state.activeUser}>
          <span class="status-separator"></span>
          <span class="status-user">{state.activeUser!.display_name}</span>
        </Show>
        <span class="status-separator"></span>
        <button
          class={`statusbar-log-btn${props.logPanelOpen ? " active" : ""}`}
          onClick={() => props.onToggleLogPanel()}
          data-tooltip="Toggle log panel"
        >
          Logs ({props.logMessageCount})
        </button>
      </div>
    </footer>
  );
};

export default StatusBar;
