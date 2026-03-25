/**
 * UsersTab: User management and bulk cross-user copy operations.
 *
 * SplitPane layout:
 * - Left (40%): User management — list all users, create, rename, delete
 * - Right (60%): Bulk copy — select source/target user, preview, execute
 */

import { Component, For, Show, createSignal, createMemo, createEffect } from "solid-js";
import { state, actions, guardNavigation } from "../../stores/app";
import { api } from "../../api/client";
import type { CopySummary, User } from "../../api/types";
import SplitPane from "../SplitPane";
import "./UsersTab.css";

interface UsersTabProps {
  onUserSwitch: (userId: number) => Promise<void>;
}

const UsersTab: Component<UsersTabProps> = (props) => {
  // ---------------------------------------------------------------------------
  // User management state
  // ---------------------------------------------------------------------------
  const [newUsername, setNewUsername] = createSignal("");
  const [newDisplayName, setNewDisplayName] = createSignal("");
  const [creating, setCreating] = createSignal(false);

  const [editingUserId, setEditingUserId] = createSignal<number | null>(null);
  const [editUsername, setEditUsername] = createSignal("");
  const [editDisplayName, setEditDisplayName] = createSignal("");

  const [deleteConfirmId, setDeleteConfirmId] = createSignal<number | null>(null);
  const [switching, setSwitching] = createSignal(false);

  // ---------------------------------------------------------------------------
  // User switching
  // ---------------------------------------------------------------------------
  async function handleSwitchUser(userId: number): Promise<void> {
    if (state.activeUser?.id === userId || switching()) return;
    const result = await guardNavigation();
    if (result === "cancel") return;
    if (result === "save" && state.saveHandler) {
      try { await state.saveHandler(); } catch { return; }
    }
    setSwitching(true);
    try {
      const result = await api.switchSession(userId);
      const switchedUser = result.user ?? state.users.find((u) => u.id === userId) ?? null;
      actions.setActiveUser(switchedUser);
      if (switchedUser) {
        await props.onUserSwitch(switchedUser.id);
      }
      actions.addToast("success", "User Switched", `Switched to ${switchedUser?.display_name ?? "user"}`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    } finally {
      setSwitching(false);
    }
  }

  // ---------------------------------------------------------------------------
  // Bulk copy state
  // ---------------------------------------------------------------------------
  const [sourceUserId, setSourceUserId] = createSignal<number | null>(null);
  const [targetUserId, setTargetUserId] = createSignal<number | null>(null);
  const [copying, setCopying] = createSignal(false);
  const [copyResult, setCopyResult] = createSignal<CopySummary | null>(null);
  const [confirmCopy, setConfirmCopy] = createSignal(false);

  // Source user counts (loaded on selection)
  const [sourceCounts, setSourceCounts] = createSignal<{
    prompts: number; scripts: number; chains: number;
  } | null>(null);

  const canBulkCopy = createMemo(() =>
    sourceUserId() !== null &&
    targetUserId() !== null &&
    sourceUserId() !== targetUserId() &&
    !copying()
  );

  // Load counts when source user changes
  createEffect(() => {
    const sid = sourceUserId();
    if (sid === null) {
      setSourceCounts(null);
      return;
    }
    loadSourceCounts(sid);
  });

  async function loadSourceCounts(userId: number): Promise<void> {
    try {
      const [promptPage, scriptPage, chainPage] = await Promise.all([
        api.listPrompts({ user_id: userId }),
        api.listScripts({ user_id: userId }),
        api.listChains({ user_id: userId }),
      ]);
      setSourceCounts({
        prompts: promptPage.total,
        scripts: scriptPage.total,
        chains: chainPage.total,
      });
    } catch {
      setSourceCounts(null);
    }
  }

  // ---------------------------------------------------------------------------
  // User CRUD
  // ---------------------------------------------------------------------------
  async function handleCreateUser(): Promise<void> {
    const username = newUsername().trim();
    const displayName = newDisplayName().trim();
    if (!username || !displayName) return;

    setCreating(true);
    try {
      const user = await api.createUser(username, displayName);
      actions.setUsers([...state.users, user]);
      setNewUsername("");
      setNewDisplayName("");
      actions.addToast("success", "User Created", `"${user.display_name}" created`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  }

  function startEditing(user: User): void {
    setEditingUserId(user.id);
    setEditUsername(user.username);
    setEditDisplayName(user.display_name);
  }

  async function handleSaveEdit(): Promise<void> {
    const userId = editingUserId();
    if (userId === null) return;

    try {
      const updated = await api.updateUser(userId, editDisplayName().trim(), editUsername().trim());
      actions.setUsers(state.users.map((u) => (u.id === userId ? updated : u)));
      if (state.activeUser?.id === userId) {
        actions.setActiveUser(updated);
      }
      setEditingUserId(null);
      actions.addToast("success", "Updated", `User "${updated.display_name}" updated`);
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  function cancelEdit(): void {
    setEditingUserId(null);
  }

  async function handleDeleteUser(userId: number): Promise<void> {
    // Guard: never allow deleting the active user
    if (state.activeUser?.id === userId) {
      actions.addToast("error", "Error", "Cannot delete the active user. Switch to another user first.");
      setDeleteConfirmId(null);
      return;
    }
    try {
      await api.deleteUser(userId);
      actions.setUsers(state.users.filter((u) => u.id !== userId));
      setDeleteConfirmId(null);
      // Clear source/target if the deleted user was selected
      if (sourceUserId() === userId) setSourceUserId(null);
      if (targetUserId() === userId) setTargetUserId(null);
      actions.addToast("success", "Deleted", "User deleted");
    } catch (err) {
      actions.addToast("error", "Error", err instanceof Error ? err.message : String(err));
    }
  }

  // ---------------------------------------------------------------------------
  // Bulk copy
  // ---------------------------------------------------------------------------
  async function executeBulkCopy(): Promise<void> {
    const sid = sourceUserId();
    const tid = targetUserId();
    if (sid === null || tid === null) return;

    setConfirmCopy(false);
    setCopying(true);
    setCopyResult(null);
    try {
      const result = await api.bulkCopyAll(sid, tid);
      setCopyResult(result);
      actions.addToast("success", "Copy Complete",
        `${result.prompts_copied} prompts, ${result.scripts_copied} scripts, ${result.chains_copied} chains copied`);
    } catch (err) {
      actions.addToast("error", "Copy Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setCopying(false);
    }
  }

  function sourceUser() {
    return state.users.find((u) => u.id === sourceUserId()) ?? null;
  }

  function targetUser() {
    return state.users.find((u) => u.id === targetUserId()) ?? null;
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------
  return (
    <SplitPane
      storageKey="users"
      defaultRatio={0.4}
      minLeftPx={320}
      minRightPx={400}
      left={
        <div class="users-left">
          <div class="section-header">
            <h2>User Management</h2>
            <span class="badge">{state.users.length}</span>
          </div>

          {/* User list */}
          <div class="user-list">
            <For each={state.users}>
              {(user) => (
                <div
                  class={`user-card${state.activeUser?.id === user.id ? " active" : ""}`}
                >
                  <Show
                    when={editingUserId() !== user.id}
                    fallback={
                      <div class="user-card-editing">
                        <div class="edit-row">
                          <label>Display Name</label>
                          <input
                            type="text"
                            value={editDisplayName()}
                            onInput={(e) => setEditDisplayName(e.currentTarget.value)}
                            onKeyDown={(e) => { if (e.key === "Enter") handleSaveEdit(); if (e.key === "Escape") cancelEdit(); }}
                            data-tooltip="Edit display name — press Enter to save, Escape to cancel"
                          />
                        </div>
                        <div class="edit-row">
                          <label>Username</label>
                          <input
                            type="text"
                            value={editUsername()}
                            onInput={(e) => setEditUsername(e.currentTarget.value)}
                            onKeyDown={(e) => { if (e.key === "Enter") handleSaveEdit(); if (e.key === "Escape") cancelEdit(); }}
                            data-tooltip="Edit username — lowercase, letters, numbers, underscores only"
                          />
                        </div>
                        <div class="edit-actions">
                          <button class="btn btn-primary btn-sm" onClick={handleSaveEdit} title="Save user changes">Save</button>
                          <button class="btn btn-secondary btn-sm" onClick={cancelEdit} title="Discard changes">Cancel</button>
                        </div>
                      </div>
                    }
                  >
                    <div class="user-card-info">
                      <div class="user-card-name">
                        {user.display_name}
                        <Show when={state.activeUser?.id === user.id}>
                          <span class="active-badge">Active</span>
                        </Show>
                      </div>
                      <div class="user-card-username">@{user.username}</div>
                      <Show when={user.created_at}>
                        <div class="user-card-date">Created: {new Date(user.created_at!).toLocaleDateString()}</div>
                      </Show>
                    </div>
                    <div class="user-card-actions">
                      <Show when={state.activeUser?.id !== user.id}>
                        <button
                          class="btn btn-primary btn-sm"
                          onClick={() => handleSwitchUser(user.id)}
                          disabled={switching()}
                          attr:data-tooltip={"Switch active user to " + user.display_name}
                        >
                          {switching() ? "..." : "Switch"}
                        </button>
                      </Show>
                      <button
                        class="btn btn-secondary btn-sm"
                        onClick={() => startEditing(user)}
                        attr:data-tooltip={"Edit " + user.display_name}
                      >
                        Edit
                      </button>
                      <Show when={state.activeUser?.id !== user.id}>
                        <Show
                          when={deleteConfirmId() !== user.id}
                          fallback={
                            <div class="delete-confirm">
                              <span>Delete?</span>
                              <button class="btn btn-danger btn-sm" onClick={() => handleDeleteUser(user.id)} title="Confirm deletion — this cannot be undone">Yes</button>
                              <button class="btn btn-secondary btn-sm" onClick={() => setDeleteConfirmId(null)} title="Cancel deletion">No</button>
                            </div>
                          }
                        >
                          <button
                            class="btn btn-danger btn-sm"
                            onClick={() => setDeleteConfirmId(user.id)}
                            data-tooltip="Delete this user and all their data"
                          >
                            Delete
                          </button>
                        </Show>
                      </Show>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>

          {/* Create new user form */}
          <div class="create-user-section">
            <h3>Create New User</h3>
            <div class="create-form">
              <div class="form-row">
                <label>Username</label>
                <input
                  type="text"
                  placeholder="lowercase, a-z, 0-9, _"
                  value={newUsername()}
                  onInput={(e) => setNewUsername(e.currentTarget.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") handleCreateUser(); }}
                  data-tooltip="Lowercase letters, numbers, and underscores only"
                />
              </div>
              <div class="form-row">
                <label>Display Name</label>
                <input
                  type="text"
                  placeholder="Full name or nickname"
                  value={newDisplayName()}
                  onInput={(e) => setNewDisplayName(e.currentTarget.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") handleCreateUser(); }}
                  data-tooltip="How this user appears in the interface"
                />
              </div>
              <button
                class="btn btn-primary"
                onClick={handleCreateUser}
                disabled={creating() || !newUsername().trim() || !newDisplayName().trim()}
                data-tooltip="Create a new user account"
              >
                {creating() ? "Creating..." : "Create User"}
              </button>
            </div>
          </div>
        </div>
      }
      right={
        <div class="users-right">
          <div class="section-header">
            <h2>Bulk Copy</h2>
          </div>

          {/* Source / Target selection */}
          <div class="copy-selector">
            <div class="copy-selector-row">
              <div class="copy-field">
                <label>From (Source):</label>
                <select
                  value={sourceUserId() ?? ""}
                  onChange={(e) => {
                    const val = e.currentTarget.value;
                    const newSourceId = val ? Number(val) : null;
                    setSourceUserId(newSourceId);
                    setCopyResult(null);
                    // Clear target if it now matches source
                    if (newSourceId !== null && targetUserId() === newSourceId) {
                      setTargetUserId(null);
                    }
                  }}
                  data-tooltip="Select the user whose content you want to copy"
                >
                  <option value="">-- Select user --</option>
                  <For each={state.users}>
                    {(user) => <option value={user.id}>{user.display_name} (@{user.username})</option>}
                  </For>
                </select>
              </div>

              <div class="copy-arrow">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                  <path d="M5 12h14M13 6l6 6-6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
              </div>

              <div class="copy-field">
                <label>To (Target):</label>
                <select
                  value={targetUserId() ?? ""}
                  onChange={(e) => {
                    const val = e.currentTarget.value;
                    setTargetUserId(val ? Number(val) : null);
                    setCopyResult(null);
                  }}
                  data-tooltip="Select the user to receive the copied content"
                >
                  <option value="">-- Select user --</option>
                  <For each={state.users}>
                    {(user) => (
                      <option
                        value={user.id}
                        disabled={user.id === sourceUserId()}
                      >
                        {user.display_name} (@{user.username})
                        {user.id === sourceUserId() ? " (source)" : ""}
                      </option>
                    )}
                  </For>
                </select>
              </div>
            </div>
          </div>

          {/* Preview */}
          <Show when={sourceUserId() !== null && sourceCounts()}>
            <div class="copy-preview">
              <h3>Source Content: {sourceUser()?.display_name}</h3>
              <div class="copy-counts">
                <div class="count-item">
                  <span class="count-number">{sourceCounts()!.prompts}</span>
                  <span class="count-label">Prompts</span>
                </div>
                <div class="count-item">
                  <span class="count-number">{sourceCounts()!.scripts}</span>
                  <span class="count-label">Scripts</span>
                </div>
                <div class="count-item">
                  <span class="count-number">{sourceCounts()!.chains}</span>
                  <span class="count-label">Chains</span>
                </div>
              </div>
              <Show when={sourceUserId() === targetUserId()}>
                <div class="copy-warning">Source and target user cannot be the same.</div>
              </Show>
            </div>
          </Show>

          {/* Copy button + confirmation */}
          <Show when={canBulkCopy()}>
            <div class="copy-action">
              <Show
                when={!confirmCopy()}
                fallback={
                  <div class="copy-confirm-dialog">
                    <p>
                      Copy all content from <strong>{sourceUser()?.display_name}</strong> to <strong>{targetUser()?.display_name}</strong>?
                    </p>
                    <p class="copy-confirm-detail">
                      This will create copies of {sourceCounts()?.prompts ?? 0} prompts, {sourceCounts()?.scripts ?? 0} scripts,
                      and {sourceCounts()?.chains ?? 0} chains. Items with identical title and content will be skipped.
                    </p>
                    <div class="copy-confirm-actions">
                      <button class="btn btn-primary" onClick={executeBulkCopy} title="Execute the bulk copy operation">
                        Confirm Copy
                      </button>
                      <button class="btn btn-secondary" onClick={() => setConfirmCopy(false)} title="Cancel bulk copy">
                        Cancel
                      </button>
                    </div>
                  </div>
                }
              >
                <button
                  class="btn btn-primary btn-lg"
                  onClick={() => setConfirmCopy(true)}
                  disabled={copying()}
                  data-tooltip="Copy all prompts, scripts, and chains from source to target user"
                >
                  {copying() ? "Copying..." : "Copy All Content"}
                </button>
              </Show>
            </div>
          </Show>

          {/* Only-one-user hint */}
          <Show when={state.users.length < 2}>
            <div class="copy-hint">
              <p>Create at least two users to enable content copying between them.</p>
            </div>
          </Show>

          {/* Results */}
          <Show when={copyResult()}>
            {(result) => (
              <div class="copy-results">
                <h3>Copy Results</h3>
                <div class="result-grid">
                  <Show when={result().prompts_copied > 0}>
                    <div class="result-item success">{result().prompts_copied} prompt(s) copied</div>
                  </Show>
                  <Show when={result().scripts_copied > 0}>
                    <div class="result-item success">{result().scripts_copied} script(s) copied</div>
                  </Show>
                  <Show when={result().chains_copied > 0}>
                    <div class="result-item success">{result().chains_copied} chain(s) copied</div>
                  </Show>
                  <Show when={result().tags_created > 0}>
                    <div class="result-item info">{result().tags_created} tag(s) created</div>
                  </Show>
                  <Show when={result().categories_created > 0}>
                    <div class="result-item info">{result().categories_created} category(ies) created</div>
                  </Show>
                  <Show when={result().collections_created > 0}>
                    <div class="result-item info">{result().collections_created} collection(s) created</div>
                  </Show>
                  <Show when={result().skipped.length > 0}>
                    <div class="result-item warning">{result().skipped.length} item(s) skipped</div>
                    <div class="skipped-details">
                      <For each={result().skipped}>
                        {(item) => (
                          <div class="skipped-item">
                            <span class="skipped-type">{item.entity_type}</span>
                            <span class="skipped-title">"{item.title}"</span>
                            <span class="skipped-reason">{item.reason}</span>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </div>
      }
    />
  );
};

export default UsersTab;
