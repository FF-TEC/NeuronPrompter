/**
 * SSE (Server-Sent Events) client for real-time event subscriptions.
 * Connects to the NeuronPrompter SSE endpoints and dispatches typed event
 * payloads to registered handlers.
 *
 * Each SSE connection uses exponential backoff reconnection: when the
 * connection drops, the client waits an increasing delay (1s, 2s, 4s, ...,
 * up to 30s) before retrying. A successful message resets the retry counter.
 *
 * Two SSE streams are available:
 *   - /api/v1/events/logs   -> "log" events
 *   - /api/v1/events/models -> "ollama_pull_progress", "ollama_pull_complete",
 *                              "ollama_pull_error" events
 */

/** Base path for SSE event endpoints. */
const SSE_BASE = "/api/v1/events";

// ---------------------------------------------------------------------------
// Runtime type guards
// ---------------------------------------------------------------------------

function parseJson(data: string): unknown | null {
  try {
    return JSON.parse(data);
  } catch {
    return null;
  }
}

function isObject(d: unknown): d is Record<string, unknown> {
  return typeof d === "object" && d !== null;
}

function isLogPayload(d: unknown): d is { level: string; target: string; message: string } {
  return isObject(d) && "level" in d && "target" in d && "message" in d;
}

function isOllamaPullProgress(d: unknown): d is {
  model: string;
  status: string;
  total: number | null;
  completed: number | null;
} {
  return (
    isObject(d) &&
    "model" in d &&
    typeof d.model === "string" &&
    "status" in d &&
    typeof d.status === "string"
  );
}

function hasOllamaModel(d: unknown): d is { model: string } {
  return isObject(d) && "model" in d && typeof d.model === "string";
}

function isOllamaPullError(d: unknown): d is { model: string; error: string } {
  return (
    isObject(d) &&
    "model" in d &&
    typeof d.model === "string" &&
    "error" in d &&
    typeof d.error === "string"
  );
}

// ---------------------------------------------------------------------------
// Resilient SSE connection with exponential backoff
// ---------------------------------------------------------------------------

interface ResilientSSEOptions {
  setupListeners: (es: EventSource, resetRetry: () => void) => void;
  onDisconnect?: () => void;
  onOpen?: () => void;
}

/**
 * Creates an EventSource connection with automatic reconnection using
 * exponential backoff. Returns a cleanup function that closes the connection.
 */
/** Maximum number of consecutive SSE reconnection attempts before giving up. */
const MAX_RETRIES = 100;

function createResilientSSE(
  url: string,
  options: ResilientSSEOptions,
): () => void {
  let es: EventSource | null = null;
  let retryCount = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  function connect(): void {
    if (disposed) return;

    es = new EventSource(url);

    const resetRetry = (): void => {
      retryCount = 0;
    };

    if (options.onOpen) {
      es.onopen = options.onOpen;
    }

    options.setupListeners(es, resetRetry);

    es.onerror = () => {
      es?.close();
      es = null;
      if (disposed) return;

      options.onDisconnect?.();

      // L-51: Stop reconnecting after MAX_RETRIES consecutive failures to
      // prevent infinite retry loops when the server is permanently unreachable.
      if (retryCount >= MAX_RETRIES) {
        console.warn(
          `SSE connection to ${url} exceeded ${MAX_RETRIES} retries. Giving up.`,
        );
        return;
      }

      const delay = Math.min(1000 * Math.pow(2, retryCount), 30000);
      retryCount++;
      console.warn(
        `SSE connection to ${url} lost. Reconnecting in ${delay}ms (attempt ${retryCount}).`,
      );
      timer = setTimeout(connect, delay);
    };
  }

  connect();

  return () => {
    disposed = true;
    es?.close();
    es = null;
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
  };
}

// ---------------------------------------------------------------------------
// Core subscription function
// ---------------------------------------------------------------------------

/**
 * Subscribes to an SSE stream and dispatches validated JSON payloads.
 */
export function subscribeSSE(
  endpoint: string,
  handlers: Record<string, (data: unknown) => void>,
  onOpen?: () => void,
): () => void {
  const url = `${SSE_BASE}/${endpoint}`;

  return createResilientSSE(url, {
    setupListeners: (es, resetRetry) => {
      for (const [eventType, handler] of Object.entries(handlers)) {
        es.addEventListener(eventType, ((event: MessageEvent) => {
          resetRetry();
          const data = parseJson(event.data);
          if (data === null) {
            console.warn(
              `SSE [${endpoint}/${eventType}]: received malformed JSON, discarding event.`,
            );
            return;
          }
          handler(data);
        }) as EventListener);
      }
    },
    onOpen,
  });
}

// ---------------------------------------------------------------------------
// Typed subscription helpers
// ---------------------------------------------------------------------------

/**
 * Subscribes to the log stream. Each "log" SSE event carries a JSON object
 * with `level`, `target`, and `message` fields.
 */
export function subscribeToLogs(
  onLog: (entry: { level: string; target: string; message: string; timestamp: string }) => void,
): () => void {
  return subscribeSSE("logs", {
    log: (data) => {
      const now = new Date().toISOString();
      if (isLogPayload(data)) {
        onLog({
          level: (data.level || "INFO").toUpperCase(),
          target: data.target || "",
          message: data.message || "",
          timestamp: now,
        });
      } else if (typeof data === "string") {
        onLog({ level: "INFO", target: "", message: data, timestamp: now });
      } else {
        console.warn("SSE [logs/log]: payload failed type guard, discarding.", data);
      }
    },
  });
}

/**
 * Subscribes to the model events stream. Receives Ollama pull lifecycle events.
 */
export function subscribeToModels(handlers: {
  onOllamaPullProgress?: (data: { model: string; status: string; total: number | null; completed: number | null }) => void;
  onOllamaPullComplete?: (data: { model: string }) => void;
  onOllamaPullError?: (data: { model: string; error: string }) => void;
}): () => void {
  const eventHandlers: Record<string, (data: unknown) => void> = {};

  if (handlers.onOllamaPullProgress) {
    const cb = handlers.onOllamaPullProgress;
    eventHandlers["ollama_pull_progress"] = (data) => {
      if (isOllamaPullProgress(data)) {
        cb(data);
      } else {
        console.warn("SSE [models/ollama_pull_progress]: payload failed type guard.", data);
      }
    };
  }
  if (handlers.onOllamaPullComplete) {
    const cb = handlers.onOllamaPullComplete;
    eventHandlers["ollama_pull_complete"] = (data) => {
      if (hasOllamaModel(data)) {
        cb(data);
      } else {
        console.warn("SSE [models/ollama_pull_complete]: payload failed type guard.", data);
      }
    };
  }
  if (handlers.onOllamaPullError) {
    const cb = handlers.onOllamaPullError;
    eventHandlers["ollama_pull_error"] = (data) => {
      if (isOllamaPullError(data)) {
        cb(data);
      } else {
        console.warn("SSE [models/ollama_pull_error]: payload failed type guard.", data);
      }
    };
  }

  return subscribeSSE("models", eventHandlers);
}
