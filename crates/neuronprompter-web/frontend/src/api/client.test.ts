/**
 * Tests for API client utilities and bug fixes.
 *
 * Bug 1: DEFAULT_PROMPT_CONTENT must be non-empty (backend rejects empty content).
 * Bug 4: writeToSystemClipboard must copy via execCommand fallback.
 * Bug 5: toggleArchive / toggleFavorite payloads must use { value: bool }.
 */

import { describe, it, expect, vi } from "vitest";
import { DEFAULT_PROMPT_CONTENT } from "./types";
import { writeToSystemClipboard, extractTemplateVariables } from "./client";
import { readFileSync } from "fs";

// ---------------------------------------------------------------------------
// Bug 1: New Prompt default content
// ---------------------------------------------------------------------------
describe("Bug 1 – DEFAULT_PROMPT_CONTENT", () => {
  it("is defined and non-empty", () => {
    expect(DEFAULT_PROMPT_CONTENT).toBeDefined();
    expect(DEFAULT_PROMPT_CONTENT.length).toBeGreaterThan(0);
  });

  it("is a plain string (not whitespace-only)", () => {
    expect(DEFAULT_PROMPT_CONTENT.trim().length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// Bug 4: Clipboard utility
// ---------------------------------------------------------------------------
describe("Bug 4 – writeToSystemClipboard", () => {
  it("is a synchronous function that returns a boolean", () => {
    // The function must be synchronous to stay within the user-gesture context
    expect(typeof writeToSystemClipboard).toBe("function");
    // In a Node test environment (no DOM), it returns false (no clipboard API)
    const result = writeToSystemClipboard("test");
    expect(typeof result).toBe("boolean");
  });

  it("uses both execCommand and Clipboard API tiers", () => {
    const src = readFileSync(new URL("./client.ts", import.meta.url), "utf-8");
    // Extract just the writeToSystemClipboard function body
    const fnStart = src.indexOf("export function writeToSystemClipboard");
    expect(fnStart).toBeGreaterThan(-1);
    const fnBody = src.slice(fnStart, fnStart + 1000);
    expect(fnBody).toContain("execCommand");
    expect(fnBody).toContain("navigator.clipboard");
  });
});

// ---------------------------------------------------------------------------
// Bug 4 – extractTemplateVariables (client-side variable detection)
// ---------------------------------------------------------------------------
describe("Bug 4 – extractTemplateVariables", () => {
  it("returns empty array for plain text", () => {
    expect(extractTemplateVariables("Hello world")).toEqual([]);
  });

  it("extracts single variable", () => {
    expect(extractTemplateVariables("Hello {{name}}!")).toEqual(["name"]);
  });

  it("extracts multiple unique variables", () => {
    expect(extractTemplateVariables("{{a}} and {{b}} and {{c}}")).toEqual(["a", "b", "c"]);
  });

  it("deduplicates repeated variables", () => {
    expect(extractTemplateVariables("{{x}} {{x}} {{x}}")).toEqual(["x"]);
  });

  it("does NOT match whitespace inside braces (matches backend Rust regex)", () => {
    // The regex /\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g requires identifiers
    // directly adjacent to the braces -- no whitespace allowed.
    expect(extractTemplateVariables("{{ name }}")).toEqual([]);
    expect(extractTemplateVariables("{{name}}")).toEqual(["name"]);
  });

  it("returns empty for empty string", () => {
    expect(extractTemplateVariables("")).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Bug 5: Archive / Favorite payload field name
// ---------------------------------------------------------------------------
describe("Bug 5 – toggleArchive / toggleFavorite payload", () => {
  const src = readFileSync(new URL("./client.ts", import.meta.url), "utf-8");

  it("toggleArchive sends { value } not { is_archived }", () => {
    const lines = src.split("\n");
    const idx = lines.findIndex((l: string) => l.includes("toggleArchive:"));
    expect(idx).toBeGreaterThan(-1);
    const context = lines.slice(idx, idx + 3).join(" ");
    expect(context).toContain("value:");
    expect(context).not.toContain("is_archived");
  });

  it("toggleFavorite sends { value } not { is_favorite }", () => {
    const lines = src.split("\n");
    const idx = lines.findIndex((l: string) => l.includes("toggleFavorite:"));
    expect(idx).toBeGreaterThan(-1);
    const context = lines.slice(idx, idx + 3).join(" ");
    expect(context).toContain("value:");
    expect(context).not.toContain("is_favorite");
  });
});
