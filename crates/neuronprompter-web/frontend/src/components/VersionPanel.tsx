import { logError } from "../utils/errors";
import { Component, Show, For, createSignal, createEffect } from "solid-js";
import { api } from "../api/client";
import type { PromptVersion, ScriptVersion } from "../api/types";
import "./VersionPanel.css";

/**
 * Version type that represents the common fields shared between
 * PromptVersion and ScriptVersion. Both types carry id, version_number,
 * title, content, and created_at, so this union covers display needs.
 */
type VersionEntry = PromptVersion | ScriptVersion;

/**
 * Props for the VersionPanel component.
 *
 * entityId: the numeric ID of the prompt or script whose versions are displayed.
 * entityType: determines which API endpoints to call -- "prompt" uses the
 *   prompt version endpoints, "script" uses the script version endpoints.
 * currentVersion: the version number currently active in the editor.
 * onRestore: callback fired after a version restore completes successfully.
 */
interface VersionPanelProps {
  entityId: number;
  entityType: "prompt" | "script";
  currentVersion: number;
  onRestore: () => void;
}

/**
 * VersionPanel: collapsible panel showing the version history for a prompt
 * or script. Allows previewing past versions and restoring them. The panel
 * branches API calls based on entityType to hit the correct backend endpoints.
 */
const VersionPanel: Component<VersionPanelProps> = (props) => {
  const [versions, setVersions] = createSignal<VersionEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [expanded, setExpanded] = createSignal(false);
  const [selectedVersion, setSelectedVersion] = createSignal<VersionEntry | null>(null);
  const [restoring, setRestoring] = createSignal(false);

  /** Reloads versions whenever the panel is expanded and a valid entityId is present. */
  createEffect(() => {
    if (expanded() && props.entityId) {
      loadVersions();
    }
  });

  /**
   * Fetches the version list from the backend. Branches on entityType to
   * call the prompt-specific or script-specific listing endpoint.
   */
  const loadVersions = async () => {
    setLoading(true);
    try {
      const result = props.entityType === "prompt"
        ? await api.listVersions(props.entityId)
        : await api.listScriptVersions(props.entityId);
      setVersions(result);
    } catch (err) {
      logError("VersionPanel.loadVersions", err);
    } finally {
      setLoading(false);
    }
  };

  /**
   * Restores a specific version by number. Branches on entityType to call
   * the prompt-specific or script-specific restore endpoint.
   */
  const handleRestore = async (versionNumber: number) => {
    setRestoring(true);
    try {
      if (props.entityType === "prompt") {
        await api.restoreVersion(props.entityId, versionNumber);
      } else {
        await api.restoreScriptVersion(props.entityId, versionNumber);
      }
      setSelectedVersion(null);
      props.onRestore();
    } catch (err) {
      logError("VersionPanel.restore", err);
    } finally {
      setRestoring(false);
    }
  };

  /** Formats an ISO timestamp into a short locale string (e.g. "Mar 24, 10:30 AM"). */
  const formatTime = (iso: string): string => {
    return new Date(iso).toLocaleString("en-US", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  return (
    <div class="version-panel">
      <button class="version-toggle" onClick={() => setExpanded(!expanded())} title="Show/hide version history for this item">
        <svg
          width="14"
          height="14"
          viewBox="0 0 14 14"
          fill="none"
          class={expanded() ? "rotated" : ""}
        >
          <path d="M5 3l4 4-4 4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span>Version History</span>
        <Show when={props.currentVersion > 1}>
          <span class="version-count">{props.currentVersion}</span>
        </Show>
      </button>

      <Show when={expanded()}>
        <div class="version-list">
          <Show when={!loading()} fallback={<p class="version-loading">Loading...</p>}>
            <Show
              when={versions().length > 0}
              fallback={<p class="version-empty">No version history</p>}
            >
              <For each={versions()}>
                {(version) => (
                  <div class={`version-item${selectedVersion()?.id === version.id ? " active" : ""}`}>
                    <div class="version-header">
                      <span class="version-number">v{version.version_number}</span>
                      <span class="version-time">{formatTime(version.created_at)}</span>
                    </div>
                    <p class="version-title">{version.title}</p>

                    <Show when={selectedVersion()?.id === version.id}>
                      <div class="version-preview">
                        <pre class="version-content">{version.content}</pre>
                        <div class="version-actions">
                          <button
                            class="btn-restore"
                            onClick={() => handleRestore(version.version_number)}
                            disabled={restoring()}
                            data-tooltip="Replace current content with this version"
                          >
                            {restoring() ? "Restoring..." : "Restore this version"}
                          </button>
                          <button class="btn-cancel" onClick={() => setSelectedVersion(null)} title="Close version preview">
                            Cancel
                          </button>
                        </div>
                      </div>
                    </Show>

                    <Show when={selectedVersion()?.id !== version.id && version.version_number < props.currentVersion}>
                      <button class="btn-preview" onClick={() => setSelectedVersion(version)} title="Preview this version's content">
                        Preview
                      </button>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default VersionPanel;
