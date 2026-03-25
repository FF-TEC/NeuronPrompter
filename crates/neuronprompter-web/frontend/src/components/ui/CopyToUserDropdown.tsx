/**
 * Reusable dropdown button for copying an entity (prompt, script, chain) to another user.
 * Shows a list of all users except the current active user. On selection,
 * triggers the appropriate copy API call and shows a toast with the result.
 */

import { Component, For, Show, createSignal, createMemo, onMount, onCleanup } from "solid-js";
import { state, actions } from "../../stores/app";
import { logError } from "../../utils/errors";
import { api } from "../../api/client";
import type { CopySummary } from "../../api/types";
import "./CopyToUserDropdown.css";

interface CopyToUserDropdownProps {
  entityType: "prompt" | "script" | "chain";
  entityId: number;
  entityTitle: string;
}

let dropdownInstanceCounter = 0;

const CopyToUserDropdown: Component<CopyToUserDropdownProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [copying, setCopying] = createSignal(false);
  const instanceId = `copy-dropdown-${++dropdownInstanceCounter}`;

  const otherUsers = createMemo(() =>
    state.users.filter((u) => u.id !== state.activeUser?.id)
  );

  async function handleCopy(targetUserId: number): Promise<void> {
    setOpen(false);
    setCopying(true);
    try {
      let summary: CopySummary;
      switch (props.entityType) {
        case "prompt":
          summary = await api.copyPromptToUser(props.entityId, targetUserId);
          break;
        case "script":
          summary = await api.copyScriptToUser(props.entityId, targetUserId);
          break;
        case "chain":
          summary = await api.copyChainToUser(props.entityId, targetUserId);
          break;
      }

      const targetUser = state.users.find((u) => u.id === targetUserId);
      const targetName = targetUser?.display_name ?? "user";

      if (summary.skipped.length > 0 && summary.prompts_copied === 0 && summary.scripts_copied === 0 && summary.chains_copied === 0) {
        actions.addToast("info", "Already Exists",
          `"${props.entityTitle}" already exists for ${targetName}`);
      } else {
        const parts: string[] = [];
        if (summary.prompts_copied > 0) parts.push(`${summary.prompts_copied} prompt(s)`);
        if (summary.scripts_copied > 0) parts.push(`${summary.scripts_copied} script(s)`);
        if (summary.chains_copied > 0) parts.push(`${summary.chains_copied} chain(s)`);
        if (summary.tags_created > 0) parts.push(`${summary.tags_created} tag(s) created`);

        actions.addToast("success", "Copied",
          `${parts.join(", ")} copied to ${targetName}`);

        if (summary.skipped.length > 0) {
          actions.addToast("info", "Some Skipped",
            `${summary.skipped.length} item(s) already existed and were skipped`);
        }
      }
    } catch (err) {
      logError("CopyToUserDropdown.copy", err);
      actions.addToast("error", "Copy Failed",
        err instanceof Error ? err.message : String(err));
    } finally {
      setCopying(false);
    }
  }

  // Close dropdown when clicking outside this specific instance
  function handleClickOutside(e: MouseEvent): void {
    const target = e.target as HTMLElement;
    if (!target.closest(`#${instanceId}`)) {
      setOpen(false);
    }
  }

  onMount(() => window.addEventListener("click", handleClickOutside));
  onCleanup(() => window.removeEventListener("click", handleClickOutside));

  return (
    <div class="copy-to-user-dropdown" id={instanceId}>
      <button
        class="btn btn-secondary"
        onClick={() => setOpen(!open())}
        disabled={copying() || otherUsers().length === 0}
        attr:data-tooltip={otherUsers().length === 0
          ? "No other users available. Create another user in the Users tab."
          : `Copy this ${props.entityType} to another user`}
        aria-expanded={open()}
        aria-haspopup="menu"
      >
        {copying() ? "Copying..." : "Copy to..."}
        <Show when={!copying()}>
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" style="margin-left: 4px">
            <path d="M2 4l3 3 3-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </Show>
      </button>

      <Show when={open()}>
        <div class="copy-dropdown-menu" role="menu">
          <div class="copy-dropdown-header">Copy to user:</div>
          <For each={otherUsers()}>
            {(user) => (
              <button
                class="copy-dropdown-item"
                role="menuitem"
                onClick={() => handleCopy(user.id)}
                attr:data-tooltip={"Copy to " + user.display_name}
              >
                {user.display_name}
                <span class="copy-dropdown-username">@{user.username}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default CopyToUserDropdown;
