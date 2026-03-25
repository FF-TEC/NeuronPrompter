import { Component, createSignal, onMount, For, Show } from "solid-js";
import { api } from "../api/client";
import type { User } from "../api/types";

interface LoginPageProps {
  onLogin: (user: User) => void;
}

/**
 * LoginPage: session-based login screen shown when no valid session exists.
 * Lists all existing users as clickable profile buttons. When no users exist,
 * an inline creation form is displayed so the first account can be set up
 * without requiring a separate admin panel.
 */
const LoginPage: Component<LoginPageProps> = (props) => {
  const [users, setUsers] = createSignal<User[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [loggingIn, setLoggingIn] = createSignal(false);

  // M-53: Signals for the inline user creation form shown when no users exist.
  const [newUsername, setNewUsername] = createSignal("");
  const [newDisplayName, setNewDisplayName] = createSignal("");
  const [creating, setCreating] = createSignal(false);

  onMount(async () => {
    try {
      const userList = await api.listUsers();
      // Skip the selection screen when only one user exists.
      if (userList.length === 1 && userList[0]) {
        await handleLogin(userList[0].id);
        return;
      }
      setUsers(userList);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  });

  /** Creates a session for the selected user and invokes the onLogin callback.
   * The createSession endpoint returns { ok: true } with the session token set
   * as an HttpOnly cookie. After session creation, the user object is fetched
   * via sessionMe to populate the onLogin callback with the full user data. */
  async function handleLogin(userId: number): Promise<void> {
    if (loggingIn()) return;
    setLoggingIn(true);
    try {
      await api.createSession(userId);
      // Session cookie is now set. Fetch the authenticated user via sessionMe
      // to get the full user object for the onLogin callback.
      const me = await api.sessionMe();
      if (me?.user) {
        props.onLogin(me.user);
      } else {
        setError("Session was created but no user data returned.");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoggingIn(false);
    }
  }

  /**
   * M-53: Creates a user via the API and reloads the user list.
   * Displayed when the database has no users yet, so the first account
   * can be bootstrapped directly from the login screen.
   */
  async function handleCreateUser(): Promise<void> {
    const username = newUsername().trim();
    if (!username || creating()) return;
    setCreating(true);
    setError(null);
    try {
      await api.createUser(username, newDisplayName().trim() || username);
      const userList = await api.listUsers();
      setUsers(userList);
      setNewUsername("");
      setNewDisplayName("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  }

  /** Initiates a native window drag via wry IPC. The drag bar at the top of the
   * login page serves as the window handle since the Topbar is not rendered on
   * this screen. In browser mode (no wry), this is a no-op. */
  function startWindowDrag(): void {
    const w = window as unknown as { ipc?: { postMessage: (m: string) => void } };
    w.ipc?.postMessage("drag");
  }

  return (
    <>
      {/* Drag bar for native window (wry): allows moving the frameless window
          from the login screen where the Topbar is not rendered. */}
      <div class="login-drag-bar" onMouseDown={startWindowDrag} />
      {/* Close button in top-right corner so the user can exit the app
          from the login screen where the Topbar is not rendered. Rendered
          outside the .splash flex container to avoid layout interference. */}
      <button
        class="login-close-btn"
        onClick={() => {
          const w = window as unknown as { ipc?: { postMessage: (m: string) => void } };
          w.ipc?.postMessage("close");
        }}
        aria-label="Close"
      >
        &#x2715;
      </button>
      <main class="splash">
      <h1 class="splash-title">
        <span class="logo-neuron">Neuron</span>
        <span class="logo-prompter">Prompter</span>
      </h1>
      <Show when={!loading()} fallback={<p class="splash-status">Loading users...</p>}>
        <Show when={!error()} fallback={<p class="splash-error">{error()}</p>}>
          <Show
            when={users().length > 0}
            fallback={
              <div class="login-create" style={{ "max-width": "340px", margin: "0 auto", "text-align": "center" }}>
                <p class="splash-status" style={{ "margin-bottom": "12px" }}>No users found. Create your first account:</p>
                <input
                  type="text"
                  class="field-input"
                  placeholder="Username"
                  value={newUsername()}
                  onInput={(e) => setNewUsername(e.currentTarget.value)}
                  style={{ "margin-bottom": "8px", width: "100%" }}
                />
                <input
                  type="text"
                  class="field-input"
                  placeholder="Display Name (optional)"
                  value={newDisplayName()}
                  onInput={(e) => setNewDisplayName(e.currentTarget.value)}
                  style={{ "margin-bottom": "12px", width: "100%" }}
                />
                <button
                  class="btn btn-primary"
                  onClick={handleCreateUser}
                  disabled={!newUsername().trim() || creating()}
                >
                  {creating() ? "Creating..." : "Create User"}
                </button>
              </div>
            }
          >
            <div class="login-user-list">
              <p class="login-prompt">Select your profile:</p>
              <For each={users()}>
                {(user) => (
                  <button
                    class="login-user-btn"
                    disabled={loggingIn()}
                    onClick={() => void handleLogin(user.id)}
                  >
                    <span class="login-user-name">{user.display_name}</span>
                    <span class="login-user-username">@{user.username}</span>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </Show>
    </main>
    </>
  );
};

export default LoginPage;
