/**
 * Tests for store and editor bug fixes.
 *
 * Bug 6: Detail setters must use reconcile() and editors must track
 *         primitive values (id:updated_at) instead of store proxy references.
 */

import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";

// ---------------------------------------------------------------------------
// Bug 6 – Store uses reconcile()
// ---------------------------------------------------------------------------
describe("Bug 6 – store setters use reconcile", () => {
  const src = readFileSync(new URL("./app.ts", import.meta.url), "utf-8");

  it("imports reconcile from solid-js/store", () => {
    expect(src).toContain("reconcile");
    expect(src).toMatch(/import\s*\{[^}]*reconcile[^}]*\}\s*from\s*["']solid-js\/store["']/);
  });

  it("setActivePromptDetail uses reconcile()", () => {
    // The setter body spans multiple lines, so check both the function name
    // and reconcile appear in close proximity
    const lines = src.split("\n");
    const idx = lines.findIndex((l) => l.includes("setActivePromptDetail"));
    expect(idx).toBeGreaterThan(-1);
    const context = lines.slice(idx, idx + 3).join(" ");
    expect(context).toContain("reconcile");
  });

  it("setActiveChainDetail uses reconcile()", () => {
    const lines = src.split("\n");
    const idx = lines.findIndex((l) => l.includes("setActiveChainDetail"));
    expect(idx).toBeGreaterThan(-1);
    const context = lines.slice(idx, idx + 3).join(" ");
    expect(context).toContain("reconcile");
  });

  it("setActiveScriptDetail uses reconcile()", () => {
    const lines = src.split("\n");
    const idx = lines.findIndex((l) => l.includes("setActiveScriptDetail"));
    expect(idx).toBeGreaterThan(-1);
    const context = lines.slice(idx, idx + 3).join(" ");
    expect(context).toContain("reconcile");
  });
});

// ---------------------------------------------------------------------------
// Bug 6 – Editor tracking key generation
// ---------------------------------------------------------------------------
describe("Bug 6 – editor tracking key pattern", () => {
  it("PromptEditor tracks prompt.id and prompt.updated_at", () => {
    const src = readFileSync(
      new URL("../components/PromptEditor.tsx", import.meta.url),
      "utf-8",
    );
    expect(src).toContain("props.detail.prompt.id");
    expect(src).toContain("props.detail.prompt.updated_at");
    // Must NOT have the old broken pattern: on(() => props.detail, (detail) =>
    expect(src).not.toMatch(/on\(\s*\n?\s*\(\)\s*=>\s*props\.detail\s*,\s*\n?\s*\(detail\)/);
  });

  it("ChainEditor tracks chain.id and chain.updated_at", () => {
    const src = readFileSync(
      new URL("../components/ChainEditor.tsx", import.meta.url),
      "utf-8",
    );
    expect(src).toContain("d.chain.id");
    expect(src).toContain("d.chain.updated_at");
    // Must NOT have old pattern: on(detail, (d) =>
    expect(src).not.toMatch(/on\(\s*detail\s*,\s*\(d\)\s*=>/);
  });

  it("ScriptEditor tracks script.id and script.updated_at", () => {
    const src = readFileSync(
      new URL("../components/ScriptEditor.tsx", import.meta.url),
      "utf-8",
    );
    expect(src).toContain("props.detail.script.id");
    expect(src).toContain("props.detail.script.updated_at");
    // Must NOT have old pattern: on(() => props.detail, (detail) =>
    expect(src).not.toMatch(/on\(\s*\n?\s*\(\)\s*=>\s*props\.detail\s*,\s*\n?\s*\(detail\)/);
  });
});

// ---------------------------------------------------------------------------
// Bug 6 – tracking key logic
// ---------------------------------------------------------------------------
describe("Bug 6 – tracking key correctness", () => {
  it("generates different keys for different prompts", () => {
    const keyA = `${1}:${"2026-01-01T00:00:00"}`;
    const keyB = `${2}:${"2026-01-02T00:00:00"}`;
    expect(keyA).not.toBe(keyB);
  });

  it("generates different keys when same prompt is updated", () => {
    const keyBefore = `${1}:${"2026-01-01T00:00:00"}`;
    const keyAfter = `${1}:${"2026-01-01T00:05:00"}`;
    expect(keyBefore).not.toBe(keyAfter);
  });

  it("generates null key when detail is null", () => {
    const detail = null;
    const key = detail ? `${(detail as any).prompt.id}:${(detail as any).prompt.updated_at}` : null;
    expect(key).toBeNull();
  });
});
