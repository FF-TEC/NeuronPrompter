/**
 * Example content definitions and seeding logic for first-run setup.
 *
 * Contains the example prompts, script, and chain offered during the
 * WelcomeDialog. Seeding calls the existing CRUD API endpoints
 * sequentially. The chain references the IDs returned by the prompt
 * creation calls. The seeded flag is stored in the per-user
 * UserSettings.extra JSON field.
 */

import { api, ApiError } from "./client";
import type { Prompt, Script, Chain, NewPrompt, NewScript, NewChain } from "./types";

// ---------------------------------------------------------------------------
// Seeded flag key inside UserSettings.extra JSON
// ---------------------------------------------------------------------------

const EXTRA_KEY = "examples_seeded";
const EXTRA_IDS_KEY = "examples_ids";

// ---------------------------------------------------------------------------
// Example content definitions
// ---------------------------------------------------------------------------

/** Builds the first example prompt payload for a given user. */
function codeReviewPrompt(userId: number): NewPrompt {
  return {
    user_id: userId,
    title: "Code Review Request",
    description: "Asks an LLM to review code in a given language with specific focus areas.",
    content: [
      "Review the following {{language}} code. Focus on {{focus_area}}.",
      "",
      "```{{language}}",
      "{{code}}",
      "```",
      "",
      "Provide:",
      "1. A summary of what the code does",
      "2. Issues found (bugs, security, performance)",
      "3. Concrete suggestions for each issue",
    ].join("\n"),
    tag_ids: [],
    category_ids: [],
    collection_ids: [],
  };
}

/** Builds the second example prompt payload for a given user. */
function commitMessagePrompt(userId: number): NewPrompt {
  return {
    user_id: userId,
    title: "Commit Message Generator",
    description: "Generates a conventional commit message from a diff.",
    content: [
      "Write a commit message for the following diff. Use conventional commit format (feat:, fix:, docs:, refactor:, etc.).",
      "",
      "Repository: {{repository}}",
      "Context: {{context}}",
      "",
      "Diff:",
      "```",
      "{{diff}}",
      "```",
      "",
      "Requirements:",
      "- Subject line under 72 characters",
      "- Body explains the \"why\", not the \"what\"",
      "- Reference {{ticket_id}} if provided",
    ].join("\n"),
    tag_ids: [],
    category_ids: [],
    collection_ids: [],
  };
}

/** Builds the example script payload for a given user. */
function jsonSchemaScript(userId: number): NewScript {
  return {
    user_id: userId,
    title: "json_schema_scaffold.txt",
    script_language: "text",
    description: "A reusable scaffold for defining JSON Schema objects.",
    content: [
      "{",
      '  "$schema": "https://json-schema.org/draft/2020-12/schema",',
      '  "title": "{{schema_title}}",',
      '  "description": "{{schema_description}}",',
      '  "type": "object",',
      '  "required": [],',
      '  "properties": {}',
      "}",
    ].join("\n"),
    tag_ids: [],
    category_ids: [],
    collection_ids: [],
  };
}

/** Builds the example chain payload referencing two prompt IDs. */
function reviewThenCommitChain(
  userId: number,
  prompt1Id: number,
  prompt2Id: number,
): NewChain {
  return {
    user_id: userId,
    title: "Review Then Commit",
    description:
      "Chains the Code Review Request prompt with the Commit Message Generator. " +
      "Demonstrates how chains combine multiple prompts into a sequential workflow.",
    separator: "\n\n---\n\n",
    steps: [
      { step_type: "prompt", item_id: prompt1Id },
      { step_type: "prompt", item_id: prompt2Id },
    ],
    prompt_ids: [],
    tag_ids: [],
    category_ids: [],
    collection_ids: [],
  };
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/** Entities created by a single seeding operation. */
export interface SeedResult {
  prompts: Prompt[];
  scripts: Script[];
  chains: Chain[];
}

// ---------------------------------------------------------------------------
// Seeding
// ---------------------------------------------------------------------------

/**
 * Creates example prompts, a script, and a chain for the given user.
 *
 * Calls existing CRUD endpoints sequentially because the chain depends
 * on the IDs returned by the prompt creation calls. After all entities
 * are created, marks the user as seeded via the UserSettings.extra field.
 *
 * @param userId - The ID of the user to create examples for.
 * @returns The created entities.
 */
export async function seedExamples(userId: number): Promise<SeedResult> {
  const prompt1 = await api.createPrompt(codeReviewPrompt(userId));
  const prompt2 = await api.createPrompt(commitMessagePrompt(userId));
  const script1 = await api.createScript(jsonSchemaScript(userId));
  const chain1 = await api.createChain(
    reviewThenCommitChain(userId, prompt1.id, prompt2.id),
  );

  await markExamplesSeeded(userId, {
    prompt_ids: [prompt1.id, prompt2.id],
    script_ids: [script1.id],
    chain_ids: [chain1.id],
  });

  return {
    prompts: [prompt1, prompt2],
    scripts: [script1],
    chains: [chain1],
  };
}

// ---------------------------------------------------------------------------
// Seeded flag helpers
// ---------------------------------------------------------------------------

/**
 * Parses the extra JSON field from user settings.
 *
 * @param extra - Raw JSON string from UserSettings.extra.
 * @returns Parsed object, or empty object on parse failure.
 */
function parseExtra(extra: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(extra);
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

/**
 * Checks whether example content has been seeded for the given user.
 *
 * Reads the examples_seeded flag from UserSettings.extra.
 *
 * @param userId - The user to check.
 * @returns True if examples have been seeded previously.
 */
export async function hasExamplesBeenSeeded(userId: number): Promise<boolean> {
  const settings = await api.getUserSettings(userId);
  const extra = parseExtra(settings.extra);
  return extra[EXTRA_KEY] === true;
}

/** IDs of entities created during a single seeding operation. */
interface SeededIds {
  prompt_ids: number[];
  script_ids: number[];
  chain_ids: number[];
}

/**
 * Sets the examples_seeded flag and stores the created entity IDs in
 * UserSettings.extra for the given user.
 *
 * Merges the data into the existing extra JSON object so other keys
 * are preserved.
 *
 * @param userId - The user to mark as seeded.
 * @param ids - The IDs of the entities that were created.
 */
async function markExamplesSeeded(userId: number, ids: SeededIds): Promise<void> {
  const settings = await api.getUserSettings(userId);
  const extra = parseExtra(settings.extra);
  extra[EXTRA_KEY] = true;
  extra[EXTRA_IDS_KEY] = ids;
  await api.updateUserSettings({ ...settings, extra: JSON.stringify(extra) });
}

/**
 * Clears the examples_seeded flag and stored IDs from UserSettings.extra.
 *
 * Removes both keys from the extra JSON object so the "Add example content"
 * button becomes available again in SettingsTab.
 *
 * @param userId - The user to reset.
 */
async function clearExamplesSeededFlag(userId: number): Promise<void> {
  const settings = await api.getUserSettings(userId);
  const extra = parseExtra(settings.extra);
  delete extra[EXTRA_KEY];
  delete extra[EXTRA_IDS_KEY];
  await api.updateUserSettings({ ...settings, extra: JSON.stringify(extra) });
}

/**
 * Attempts to delete an entity by ID. Returns true if the entity was
 * deleted or did not exist (404). Returns false if deletion failed for
 * another reason (e.g. foreign key RESTRICT constraint).
 *
 * @param deleteFn - The API delete function to call.
 * @param id - The entity ID to delete.
 * @returns True if the entity is gone, false if it survived.
 */
async function tryDelete(
  deleteFn: (id: number) => Promise<void>,
  id: number,
): Promise<boolean> {
  try {
    await deleteFn(id);
    return true;
  } catch (err) {
    if (err instanceof ApiError && err.status === 404) {
      return true;
    }
    return false;
  }
}

/**
 * Deletes the example entities that were created during seeding.
 *
 * Reads the stored entity IDs from UserSettings.extra and deletes each
 * entity via the CRUD API. Entities that no longer exist (404) are
 * treated as successfully removed. Entities that cannot be deleted
 * (e.g. a prompt referenced by a user-created chain via ON DELETE
 * RESTRICT) are tracked as survivors.
 *
 * If all entities are removed, the seeded flag is cleared entirely.
 * If some entities survive, the flag and their IDs are kept so the
 * user can retry removal after resolving the reference.
 *
 * @param userId - The user whose examples should be removed.
 * @throws Error when some entities could not be deleted.
 */
export async function removeExamples(userId: number): Promise<void> {
  const settings = await api.getUserSettings(userId);
  const extra = parseExtra(settings.extra);
  const ids = extra[EXTRA_IDS_KEY] as SeededIds | undefined;

  if (!ids) {
    await clearExamplesSeededFlag(userId);
    return;
  }

  // Delete chains first (their chain_steps reference prompts/scripts
  // via ON DELETE RESTRICT, so chains must be gone before prompts).
  const survivingChains: number[] = [];
  for (const id of ids.chain_ids ?? []) {
    if (!await tryDelete(api.deleteChain.bind(api), id)) {
      survivingChains.push(id);
    }
  }

  const survivingScripts: number[] = [];
  for (const id of ids.script_ids ?? []) {
    if (!await tryDelete(api.deleteScript.bind(api), id)) {
      survivingScripts.push(id);
    }
  }

  const survivingPrompts: number[] = [];
  for (const id of ids.prompt_ids ?? []) {
    if (!await tryDelete(api.deletePrompt.bind(api), id)) {
      survivingPrompts.push(id);
    }
  }

  const hasSurvivors =
    survivingChains.length > 0 ||
    survivingScripts.length > 0 ||
    survivingPrompts.length > 0;

  if (hasSurvivors) {
    // Persist the surviving IDs so the user can retry after resolving
    // references (e.g. removing an example prompt from a custom chain).
    const remaining: SeededIds = {
      chain_ids: survivingChains,
      script_ids: survivingScripts,
      prompt_ids: survivingPrompts,
    };
    const freshSettings = await api.getUserSettings(userId);
    const freshExtra = parseExtra(freshSettings.extra);
    freshExtra[EXTRA_KEY] = true;
    freshExtra[EXTRA_IDS_KEY] = remaining;
    await api.updateUserSettings({
      ...freshSettings,
      extra: JSON.stringify(freshExtra),
    });

    const count =
      survivingChains.length + survivingScripts.length + survivingPrompts.length;
    throw new Error(
      `${count} example entit${count === 1 ? "y" : "ies"} could not be deleted ` +
      `because ${count === 1 ? "it is" : "they are"} referenced by other chains. ` +
      `Remove the reference first, then retry.`,
    );
  }

  await clearExamplesSeededFlag(userId);
}
