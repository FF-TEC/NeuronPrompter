import { Component, For, createEffect } from "solid-js";
import { state, actions } from "../../stores/app";
import type { LogEntry } from "../../stores/app";

/**
 * Log panel displaying real-time tracing log messages received via SSE.
 * Shows a monospace-font scrollable list of structured log entries with
 * color-coded level badges. Auto-scrolls to the bottom as messages arrive,
 * but only if the user has not scrolled up to inspect earlier messages.
 * The log buffer is capped at 500 messages in the store.
 */

/** Maps tracing log levels to CSS color values. */
const LEVEL_COLORS: Record<string, string> = {
  ERROR: "var(--color-error)",
  WARN: "var(--color-warning)",
  INFO: "var(--accent-cyan)",
  DEBUG: "var(--text-muted)",
  TRACE: "var(--text-muted)",
};

function levelColor(level: string): string {
  return LEVEL_COLORS[level.toUpperCase()] || "var(--text-muted)";
}

/** Formats a LogEntry timestamp (ISO 8601) into HH:MM:SS. */
function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString("en-GB", { hour12: false });
  } catch {
    return "";
  }
}

/**
 * Pixel threshold for the "near bottom" check. If the user is within
 * this many pixels of the bottom edge, auto-scroll is performed.
 */
const NEAR_BOTTOM_THRESHOLD = 80;

const LogPanel: Component = () => {
  let scrollRef: HTMLDivElement | undefined;

  /**
   * Auto-scroll to the bottom when new log messages arrive, but only if the
   * user is already near the bottom of the scroll area. This prevents
   * forcefully jumping the viewport when the user has scrolled up to read
   * earlier log entries (L-55).
   */
  createEffect(() => {
    void state.logMessages.length;
    if (scrollRef) {
      const isNearBottom =
        scrollRef.scrollHeight - scrollRef.scrollTop - scrollRef.clientHeight < NEAR_BOTTOM_THRESHOLD;
      if (isNearBottom) {
        scrollRef.scrollTop = scrollRef.scrollHeight;
      }
    }
  });

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <span style={styles.count}>
          {state.logMessages.length} messages
        </span>
        <button style={styles.clearBtn} onClick={() => actions.clearLogMessages()} data-tooltip="Clear all log messages">
          Clear
        </button>
      </div>
      <div ref={scrollRef} style={styles.scrollArea}>
        <For each={state.logMessages}>
          {(entry: LogEntry) => (
            <div style={styles.line}>
              <span style={styles.timestamp}>
                {formatTime(entry.timestamp)}
              </span>
              <span
                style={{
                  color: levelColor(entry.level),
                  "font-weight": entry.level === "ERROR" || entry.level === "WARN" ? "600" : "400",
                  "margin-right": "6px",
                  "min-width": "40px",
                  display: "inline-block",
                }}
              >
                {entry.level.padEnd(5)}
              </span>
              <span style={styles.target}>
                {entry.target}:
              </span>
              <span>{entry.message}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
};

const styles = {
  container: {
    background: "var(--bg-base)",
    "border-top": "1px solid var(--border-subtle)",
    padding: "var(--space-sm) var(--space-md)",
  } as const,
  header: {
    display: "flex",
    "justify-content": "space-between",
    "margin-bottom": "var(--space-sm)",
  } as const,
  count: {
    "font-size": "var(--font-size-xs)",
    color: "var(--text-muted)",
  } as const,
  clearBtn: {
    padding: "2px var(--space-sm)",
    border: "1px solid var(--border-subtle)",
    background: "transparent",
    color: "var(--text-muted)",
    "font-size": "11px",
    "border-radius": "4px",
    cursor: "pointer",
    "font-family": "inherit",
  } as const,
  scrollArea: {
    height: "300px",
    "overflow-y": "auto",
    "font-family": "monospace",
    "font-size": "11px",
    "line-height": "1.5",
    background: "rgba(0, 0, 0, 0.3)",
    "border-radius": "6px",
    padding: "var(--space-sm)",
    color: "var(--text-secondary)",
  } as const,
  line: {
    "white-space": "pre-wrap",
    "word-break": "break-all",
    padding: "1px 0",
  } as const,
  timestamp: {
    color: "var(--text-muted)",
    "margin-right": "6px",
  } as const,
  target: {
    color: "var(--accent-purple)",
    "margin-right": "6px",
  } as const,
};

export default LogPanel;
