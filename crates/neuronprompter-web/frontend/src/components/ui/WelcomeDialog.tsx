/**
 * WelcomeDialog: First-run setup screen shown when no user exists.
 *
 * Displays a warm welcome message emphasizing privacy, collects username
 * and display name to create the first user, and shows dependency status.
 * Includes a drag region so the native window can be moved, and window
 * controls (minimize, close) so the user is never trapped.
 */

import { Component, createSignal, onMount, Show, For } from "solid-js";
import { api } from "../../api/client";
import { seedExamples } from "../../api/examples";
import type { DependencyProbe } from "../../api/types";
import "./WelcomeDialog.css";

interface WelcomeDialogProps {
  /** Called after the user has been created and setup is marked complete. */
  onClose: () => void;
}

/** Initiates a native window drag via wry IPC (same mechanism as Topbar). */
function startWindowDrag(): void {
  window.ipc?.postMessage("drag");
}

/** Validates a username: lowercase alphanumeric and underscores only, 2-30 chars. */
function isValidUsername(value: string): boolean {
  return /^[a-z0-9_]{2,30}$/.test(value);
}

const WelcomeDialog: Component<WelcomeDialogProps> = (props) => {
  const [username, setUsername] = createSignal("");
  const [displayName, setDisplayName] = createSignal("");
  const [usernameError, setUsernameError] = createSignal("");
  const [submitError, setSubmitError] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [seedExamplesChecked, setSeedExamplesChecked] = createSignal(true);
  const [probes, setProbes] = createSignal<DependencyProbe[]>([]);
  const [probesLoading, setProbesLoading] = createSignal(true);
  const [dataDir, setDataDir] = createSignal("");

  onMount(async () => {
    try {
      const [status, doctor] = await Promise.all([
        api.setupStatus(),
        api.doctorProbes(),
      ]);
      setDataDir(status.data_dir ?? "");
      setProbes(doctor.probes);
    } catch {
      // Probes are informational — failure is non-critical
    } finally {
      setProbesLoading(false);
    }
  });

  function handleUsernameInput(value: string): void {
    const lower = value.toLowerCase().replace(/[^a-z0-9_]/g, "");
    setUsername(lower);
    setSubmitError("");

    if (lower.length > 0 && lower.length < 2) {
      setUsernameError("At least 2 characters");
    } else if (lower.length > 30) {
      setUsernameError("Maximum 30 characters");
    } else {
      setUsernameError("");
    }
  }

  function handleDisplayNameInput(value: string): void {
    setDisplayName(value);
    setSubmitError("");
  }

  function canSubmit(): boolean {
    return (
      isValidUsername(username()) &&
      displayName().trim().length > 0 &&
      !submitting()
    );
  }

  async function handleSubmit(): Promise<void> {
    if (!canSubmit()) return;

    setSubmitting(true);
    setSubmitError("");

    try {
      const user = await api.createUser(username(), displayName().trim());
      await api.createSession(user.id);

      if (seedExamplesChecked()) {
        try {
          await seedExamples(user.id);
        } catch {
          // Example seeding is non-critical; proceed with setup completion.
        }
      }

      await api.setupComplete();
      props.onClose();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (
        message.includes("409") ||
        message.toLowerCase().includes("unique") ||
        message.toLowerCase().includes("duplicate")
      ) {
        setSubmitError(
          `The username "${username()}" is already taken. Please choose a different one.`,
        );
      } else {
        setSubmitError(`Something went wrong. Please try again. (${message})`);
      }
    } finally {
      setSubmitting(false);
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === "Enter" && canSubmit()) {
      void handleSubmit();
    }
  }

  return (
    <div class="welcome-backdrop" role="dialog" aria-modal="true" aria-label="Welcome to NeuronPrompter">
      {/* Drag region + window controls at the top of the overlay */}
      <div class="welcome-titlebar" onMouseDown={startWindowDrag}>
        <div class="welcome-titlebar-controls" onMouseDown={(e) => e.stopPropagation()}>
          <button
            class="welcome-win-btn welcome-win-btn-minimize"
            onClick={() => window.ipc?.postMessage("minimize")}
            data-tooltip="Minimize"
            aria-label="Minimize"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M2 6h8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            </svg>
          </button>
          <button
            class="welcome-win-btn welcome-win-btn-close"
            onClick={() => window.ipc?.postMessage("close")}
            data-tooltip="Close"
            aria-label="Close"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            </svg>
          </button>
        </div>
      </div>

      <div class="welcome-surface">
        {/* Header */}
        <div class="welcome-header">
          <h1 class="welcome-title">Welcome to NeuronPrompter</h1>
          <p class="welcome-subtitle">
            Thank you for choosing NeuronPrompter! We're glad you're here.
          </p>
          <p class="welcome-subtitle welcome-privacy">
            Your prompts, your machine, your rules. NeuronPrompter runs
            entirely offline — no cloud sync, no telemetry, no data ever
            leaves your computer. Everything is stored locally and belongs
            to you alone.
          </p>
        </div>

        {/* User creation form */}
        <div class="welcome-section">
          <h2 class="welcome-section-title">Create your profile</h2>
          <p class="welcome-section-hint">
            Let's set up your personal workspace. You can add more users later
            if you'd like to keep separate prompt libraries.
          </p>

          <div class="welcome-form" onKeyDown={handleKeydown}>
            <div class="welcome-field">
              <label class="welcome-label" for="welcome-username">
                Username
              </label>
              <input
                id="welcome-username"
                class="welcome-input"
                classList={{ "welcome-input-error": usernameError().length > 0 }}
                type="text"
                placeholder="e.g. felix"
                value={username()}
                onInput={(e) => handleUsernameInput(e.currentTarget.value)}
                autocomplete="off"
                spellcheck={false}
                data-tooltip="Choose a unique username — lowercase letters, numbers, and underscores"
              />
              <Show when={usernameError()}>
                <span class="welcome-field-error">{usernameError()}</span>
              </Show>
              <span class="welcome-field-hint">
                Lowercase letters, numbers, and underscores
              </span>
            </div>

            <div class="welcome-field">
              <label class="welcome-label" for="welcome-displayname">
                Display name
              </label>
              <input
                id="welcome-displayname"
                class="welcome-input"
                type="text"
                placeholder="e.g. Felix"
                value={displayName()}
                onInput={(e) => handleDisplayNameInput(e.currentTarget.value)}
                autocomplete="off"
                data-tooltip="Your name as shown in the application"
              />
            </div>
          </div>
        </div>

        {/* Example content opt-in */}
        <div class="welcome-section">
          <label class="welcome-checkbox">
            <input
              type="checkbox"
              checked={seedExamplesChecked()}
              onChange={(e) => setSeedExamplesChecked(e.currentTarget.checked)}
            />
            <span>Create example content (2 prompts, 1 script, 1 chain)</span>
          </label>
          <p class="welcome-section-hint welcome-checkbox-hint">
            Demonstrates template variables, reusable prompts, and chains.
            You can remove these later or re-add them from Settings.
          </p>
        </div>

        {/* Dependency probes */}
        <div class="welcome-section">
          <h2 class="welcome-section-title">System status</h2>
          <div class="welcome-probes">
            <Show when={!probesLoading()} fallback={
              <div class="welcome-probe-loading">Checking dependencies...</div>
            }>
              <For each={probes()}>
                {(probe) => (
                  <div class="welcome-probe-row">
                    <div class="welcome-probe-status">
                      <span
                        class="welcome-probe-dot"
                        classList={{
                          "welcome-probe-dot-ok": probe.available,
                          "welcome-probe-dot-off": !probe.available,
                        }}
                      />
                      <span class="welcome-probe-name">{probe.name}</span>
                      <Show when={!probe.required}>
                        <span class="welcome-probe-badge">Optional</span>
                      </Show>
                    </div>
                    <div class="welcome-probe-detail">
                      <Show
                        when={probe.available}
                        fallback={
                          <>
                            <span class="welcome-probe-hint">{probe.hint}</span>
                            <a
                              class="welcome-probe-link"
                              href={probe.link}
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              {probe.link.replace("https://", "")}
                            </a>
                          </>
                        }
                      >
                        <span class="welcome-probe-ok-text">
                          Connected
                          <Show when={probe.model_count > 0}>
                            {` \u2014 ${probe.model_count} model${probe.model_count === 1 ? "" : "s"} available`}
                          </Show>
                        </span>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>

        {/* Data directory info */}
        <Show when={dataDir()}>
          <div class="welcome-data-dir">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
              <path
                d="M2 3.5A1.5 1.5 0 013.5 2h3.172a1.5 1.5 0 011.06.44l.828.82H12.5A1.5 1.5 0 0114 4.76V12.5a1.5 1.5 0 01-1.5 1.5h-9A1.5 1.5 0 012 12.5v-9z"
                stroke="currentColor"
                stroke-width="1.2"
              />
            </svg>
            <span class="welcome-data-path">{dataDir()}</span>
          </div>
        </Show>

        {/* Error */}
        <Show when={submitError()}>
          <div class="welcome-error">{submitError()}</div>
        </Show>

        {/* Footer */}
        <div class="welcome-footer">
          <button
            class="welcome-button"
            disabled={!canSubmit()}
            onClick={() => void handleSubmit()}
            data-tooltip="Create your user account and start using NeuronPrompter"
          >
            {submitting() ? "Setting up..." : "Get started"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default WelcomeDialog;
