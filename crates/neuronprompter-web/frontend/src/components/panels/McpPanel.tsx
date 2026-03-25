import { Component, Show, createSignal, onMount } from "solid-js";
import { state, actions } from "../../stores/app";
import { api } from "../../api/client";

/**
 * MCP registration management panel.
 * Shows registration status for Claude Code and Claude Desktop,
 * with Install/Uninstall buttons for each target.
 */

const McpPanel: Component = () => {
  const [loading, setLoading] = createSignal(false);
  const [statusMsg, setStatusMsg] = createSignal<string | null>(null);

  /** Fetches the current MCP registration status from the backend. */
  const refresh = async () => {
    try {
      const status = await api.mcpStatus();
      actions.setMcpClaudeCode(status.claude_code.registered);
      actions.setMcpClaudeDesktop(status.claude_desktop.registered);
    } catch {
      // Status endpoint may not be available yet
    }
  };

  onMount(refresh);

  /**
   * Handles install or uninstall actions for the specified MCP target.
   * Refreshes the registration status after the action completes.
   */
  const handleAction = async (target: "claude-code" | "claude-desktop", action: "install" | "uninstall") => {
    setLoading(true);
    setStatusMsg(null);
    try {
      if (action === "install") {
        await api.mcpInstall(target);
      } else {
        await api.mcpUninstall(target);
      }
      setStatusMsg(`${action === "install" ? "Installed" : "Uninstalled"} for ${target}`);
      await refresh();
    } catch (e) {
      setStatusMsg(`Failed: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={styles.container}>
      <h3 style={styles.title}>MCP Server Registration</h3>

      {/* Claude Code */}
      <div style={styles.row}>
        <div style={styles.rowLeft}>
          <div style={{
            width: "8px",
            height: "8px",
            "border-radius": "50%",
            background: state.mcpClaudeCode ? "var(--accent-purple)" : "var(--text-muted)",
          }} />
          <span style={styles.label}>Claude Code</span>
          <span style={styles.status}>
            {state.mcpClaudeCode ? "Registered" : "Not registered"}
          </span>
        </div>
        <div style={styles.rowRight}>
          {/* M-52: Replaced ternary JSX with idiomatic SolidJS Show/fallback pattern. */}
          <Show
            when={state.mcpClaudeCode}
            fallback={
              <button
                style={styles.btn}
                onClick={() => handleAction("claude-code", "install")}
                disabled={loading()}
                data-tooltip="Register NeuronPrompter as MCP server for Claude Code"
              >
                Install
              </button>
            }
          >
            <button
              style={styles.btnDanger}
              onClick={() => handleAction("claude-code", "uninstall")}
              disabled={loading()}
              data-tooltip="Unregister NeuronPrompter from Claude Code"
            >
              Uninstall
            </button>
          </Show>
        </div>
      </div>

      {/* Claude Desktop */}
      <div style={styles.row}>
        <div style={styles.rowLeft}>
          <div style={{
            width: "8px",
            height: "8px",
            "border-radius": "50%",
            background: state.mcpClaudeDesktop ? "var(--accent-purple)" : "var(--text-muted)",
          }} />
          <span style={styles.label}>Claude Desktop</span>
          <span style={styles.status}>
            {state.mcpClaudeDesktop ? "Registered" : "Not registered"}
          </span>
        </div>
        <div style={styles.rowRight}>
          {/* M-52: Replaced ternary JSX with idiomatic SolidJS Show/fallback pattern. */}
          <Show
            when={state.mcpClaudeDesktop}
            fallback={
              <button
                style={styles.btn}
                onClick={() => handleAction("claude-desktop", "install")}
                disabled={loading()}
                data-tooltip="Register NeuronPrompter as MCP server for Claude Desktop"
              >
                Install
              </button>
            }
          >
            <button
              style={styles.btnDanger}
              onClick={() => handleAction("claude-desktop", "uninstall")}
              disabled={loading()}
              data-tooltip="Unregister NeuronPrompter from Claude Desktop"
            >
              Uninstall
            </button>
          </Show>
        </div>
      </div>

      {/* M-52: Replaced ternary-guarded status message with Show component. */}
      <Show when={statusMsg()}>
        <div style={styles.statusMsg}>{statusMsg()}</div>
      </Show>
    </div>
  );
};

const styles = {
  container: {
    padding: "var(--space-md)",
    background: "var(--bg-elevated)",
    "border-radius": "8px",
  } as const,
  title: {
    "font-size": "var(--font-size-sm)",
    "font-weight": "600",
    color: "var(--text-primary)",
    "margin-bottom": "var(--space-md)",
    margin: "0 0 12px 0",
  } as const,
  row: {
    display: "flex",
    "align-items": "center",
    "justify-content": "space-between",
    padding: "10px 0",
    "border-bottom": "1px solid var(--border-subtle)",
  } as const,
  rowLeft: {
    display: "flex",
    "align-items": "center",
    gap: "var(--space-sm)",
  } as const,
  rowRight: {
    display: "flex",
    gap: "var(--space-sm)",
  } as const,
  label: {
    "font-size": "13px",
    color: "var(--text-primary)",
    "font-weight": "500",
  } as const,
  status: {
    "font-size": "var(--font-size-xs)",
    color: "var(--text-muted)",
  } as const,
  btn: {
    padding: "4px 12px",
    border: "1px solid var(--accent-purple)",
    background: "rgba(168, 85, 247, 0.1)",
    color: "var(--accent-purple)",
    "font-size": "var(--font-size-xs)",
    "border-radius": "4px",
    cursor: "pointer",
    "font-family": "inherit",
  } as const,
  btnDanger: {
    padding: "4px 12px",
    border: "1px solid var(--color-error)",
    background: "rgba(233, 69, 96, 0.1)",
    color: "var(--color-error)",
    "font-size": "var(--font-size-xs)",
    "border-radius": "4px",
    cursor: "pointer",
    "font-family": "inherit",
  } as const,
  statusMsg: {
    "margin-top": "var(--space-md)",
    "font-size": "var(--font-size-xs)",
    color: "var(--accent-cyan)",
  } as const,
};

export default McpPanel;
