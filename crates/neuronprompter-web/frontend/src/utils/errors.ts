/**
 * Centralized error logging utility.
 * Wraps console.error with a consistent format: "[context] message".
 * All error handlers in the application route through this function.
 *
 * @param context - Dot-separated identifier for the error location.
 * @param error - The error object or message.
 */
export function logError(context: string, error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[${context}] ${message}`);
}
