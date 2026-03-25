import { Component, For, Show, createSignal, createEffect, onMount } from "solid-js";
import { state, actions } from "../../stores/app";
import { api } from "../../api/client";
import "./ModelsTab.css";

/**
 * ModelsTab: Ollama model management with connection testing, installed model
 * listing, curated catalog for downloading, and pull/delete operations.
 *
 * Uses the web-specific endpoints (/api/v1/web/ollama/*) for model management
 * and the core endpoint (/api/v1/ollama/status) for connection testing.
 */

interface CatalogModel {
  name: string;
  family: string;
  params: string;
  description: string;
}

interface InstalledModel {
  name: string;
  size: number | null;
  digest: string | null;
  modified_at: string | null;
}

const ModelsTab: Component = () => {
  const [baseUrl, setBaseUrl] = createSignal("");
  const [selectedModel, setSelectedModel] = createSignal<string | null>(null);
  const [connected, setConnected] = createSignal(false);
  const [installedModels, setInstalledModels] = createSignal<InstalledModel[]>([]);
  const [catalog, setCatalog] = createSignal<CatalogModel[]>([]);
  const [testing, setTesting] = createSignal(false);
  const [pulling, setPulling] = createSignal<string | null>(null);
  const [pullProgress, setPullProgress] = createSignal<number>(0);
  const [deleting, setDeleting] = createSignal<string | null>(null);

  // Sync from store
  createEffect(() => {
    setBaseUrl(state.ollamaUrl);
    setSelectedModel(state.ollamaModel);
    setConnected(state.ollamaConnected);
  });

  // Listen for pull progress from SSE via store
  createEffect(() => {
    if (state.ollamaPullingModel) {
      setPulling(state.ollamaPullingModel);
    }
    if (state.ollamaPullProgress) {
      const { total, completed } = state.ollamaPullProgress;
      if (total > 0) {
        setPullProgress(Math.round((completed / total) * 100));
      }
    }
    if (!state.ollamaPullingModel && pulling()) {
      // Pull completed - refresh models
      setPulling(null);
      setPullProgress(0);
      void loadModels();
    }
  });

  // Load on mount
  onMount(() => {
    void checkConnection();
    void loadCatalog();
  });

  async function checkConnection(): Promise<void> {
    setTesting(true);
    try {
      const status = await api.ollamaStatus(baseUrl());
      setConnected(status.connected);
      actions.setOllamaConnected(status.connected);
      if (status.connected) {
        await loadModels();
        await persistSettings(baseUrl());
      }
    } catch {
      setConnected(false);
      actions.setOllamaConnected(false);
    } finally {
      setTesting(false);
    }
  }

  async function loadModels(): Promise<void> {
    try {
      const resp = await api.ollamaModels();
      setInstalledModels(resp.models || []);
    } catch {
      // Silently fail - connection might not be ready
    }
  }

  async function loadCatalog(): Promise<void> {
    try {
      const resp = await api.ollamaCatalog();
      setCatalog(resp.models || []);
    } catch {
      // Catalog endpoint may not be available
    }
  }

  async function handlePull(modelName: string): Promise<void> {
    setPulling(modelName);
    setPullProgress(0);
    actions.setOllamaPullingModel(modelName);
    try {
      await api.ollamaPull(modelName);
      actions.addToast("info", "Pulling", `Downloading ${modelName}...`);
    } catch (err) {
      setPulling(null);
      actions.setOllamaPullingModel(null);
      actions.addToast("error", "Pull Failed", err instanceof Error ? err.message : String(err));
    }
  }

  async function handleDelete(modelName: string): Promise<void> {
    setDeleting(modelName);
    try {
      await api.ollamaDelete(modelName);
      actions.addToast("success", "Deleted", `${modelName} removed`);
      await loadModels();
    } catch (err) {
      actions.addToast("error", "Delete Failed", err instanceof Error ? err.message : String(err));
    } finally {
      setDeleting(null);
    }
  }

  /** Persists the current base URL and selected model to user settings. */
  async function persistSettings(url?: string, model?: string | null): Promise<void> {
    if (!state.activeUser) return;
    try {
      const settings = await api.getUserSettings(state.activeUser.id);
      await api.updateUserSettings({
        ...settings,
        ollama_base_url: url ?? baseUrl(),
        ollama_model: model !== undefined ? model : selectedModel(),
      });
      actions.setOllamaUrl(url ?? baseUrl());
      actions.setOllamaModel(model !== undefined ? model : selectedModel());
    } catch (err) {
      actions.addToast("error", "Save Failed", err instanceof Error ? err.message : String(err));
    }
  }

  /** Called when a model is selected — immediately persists the choice. */
  async function handleSelectModel(modelName: string): Promise<void> {
    setSelectedModel(modelName);
    actions.setOllamaModel(modelName);
    await persistSettings(undefined, modelName);
  }

  function formatSize(bytes: number | null | undefined): string {
    if (bytes === null || bytes === undefined || bytes === 0) return "--";
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }

  const installedNames = () => new Set(installedModels().map(m => m.name));

  return (
    <div class="models-tab">
      <div class="models-container">
        {/* Connection Card */}
        <div class="glass-card">
          <h3 class="card-title">Connection</h3>
          <div class="field-group">
            <label class="field-label" for="ollama-base-url">Base URL</label>
            <input
              id="ollama-base-url"
              type="text"
              class="field-input"
              value={baseUrl()}
              onInput={(e) => setBaseUrl(e.currentTarget.value)}
              placeholder="http://localhost:11434"
              data-tooltip="Ollama server URL — default is http://localhost:11434"
            />
          </div>
          <div class="connection-row">
            <button class="btn-test" onClick={checkConnection} disabled={testing()} title="Test connection to the Ollama server and load available models">
              {testing() ? "Testing..." : "Test Connection"}
            </button>
            <div class="status-indicator">
              <span class="status-dot" classList={{ connected: connected() }} />
              <span class="status-label">{connected() ? "Connected" : "Disconnected"}</span>
            </div>
          </div>
        </div>

        {/* Pull Progress */}
        <Show when={pulling()}>
          <div class="glass-card">
            <h3 class="card-title">Downloading: {pulling()}</h3>
            <div class="progress-bar">
              <div class="progress-fill" style={{ width: `${pullProgress()}%` }} />
            </div>
            <div class="progress-label">{pullProgress()}%</div>
          </div>
        </Show>

        {/* Installed Models Card */}
        <div class="glass-card">
          <h3 class="card-title">
            Installed Models
            <Show when={installedModels().length > 0}>
              <span class="model-count">({installedModels().length})</span>
            </Show>
          </h3>

          <Show when={installedModels().length === 0}>
            <div class="empty-models">
              <p class="empty-text">
                {connected()
                  ? "No models installed. Pull one from the catalog below."
                  : "Connect to the Ollama server to see installed models."}
              </p>
            </div>
          </Show>
          <Show when={installedModels().length > 0}>
            <div class="model-list">
              <For each={installedModels()}>
                {(model) => (
                  <div class="model-row" classList={{ selected: selectedModel() === model.name }}>
                    <button
                      class="model-select-area"
                      onClick={() => handleSelectModel(model.name)}
                      attr:data-tooltip={"Select " + model.name + " as active model"}
                    >
                      <span class="model-radio" classList={{ checked: selectedModel() === model.name }} />
                      <span class="model-name">{model.name}</span>
                      <span class="model-size">{formatSize(model.size)}</span>
                    </button>
                    <button
                      class="btn-delete-small"
                      onClick={() => handleDelete(model.name)}
                      disabled={deleting() === model.name}
                      attr:data-tooltip={"Delete model " + model.name + " from Ollama"}
                    >
                      {deleting() === model.name ? "..." : "Delete"}
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>

        {/* Catalog Card */}
        <Show when={catalog().length > 0}>
          <div class="glass-card">
            <h3 class="card-title">Available Models (Catalog)</h3>
            <div class="catalog-list">
              <For each={catalog()}>
                {(entry) => {
                  const isInstalled = () => installedNames().has(entry.name);
                  return (
                    <div class="catalog-row">
                      <div class="catalog-info">
                        <span class="catalog-name">{entry.name}</span>
                        <span class="catalog-meta">{entry.family} · {entry.params}</span>
                        <span class="catalog-desc">{entry.description}</span>
                      </div>
                      <Show
                        when={!isInstalled()}
                        fallback={<span class="catalog-installed">Installed</span>}
                      >
                        <button
                          class="btn-pull"
                          onClick={() => handlePull(entry.name)}
                          disabled={pulling() !== null}
                          attr:data-tooltip={"Download and install " + entry.name}
                        >
                          Pull
                        </button>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        </Show>

      </div>
    </div>
  );
};

export default ModelsTab;
