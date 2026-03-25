/**
 * Shared utility functions used across multiple frontend components.
 */

/** Formats an ISO date string into a relative time description. */
export function formatDate(isoString: string): string {
  const date = new Date(isoString);
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffMinutes = Math.floor(diffMs / 60000);
  if (diffMinutes < 1) return "just now";
  if (diffMinutes < 60) return `${diffMinutes}m ago`;
  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/** Compares two number arrays for equality (order-independent). */
export function arraysEqual(a: number[], b: number[]): boolean {
  if (a.length !== b.length) return false;
  const sa = [...a].sort();
  const sb = [...b].sort();
  return sa.every((v, i) => v === sb[i]);
}

/** Truncates content to a maximum length with ellipsis. */
export function previewContent(content: string, maxLength: number = 120): string {
  return content.length <= maxLength ? content : content.slice(0, maxLength) + "...";
}
