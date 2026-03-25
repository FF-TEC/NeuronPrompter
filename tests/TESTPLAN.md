# NeuronPrompter - Parallel Sub-Agent Verification Protocol

Complete test coverage for the NeuronPrompter REST API (94 endpoints) and MCP toolset (23 tools). The main agent acts as a pure orchestrator: it creates shared users and seed data in a bootstrap phase, dispatches parallel sub-agents that each write their results to a dedicated .md file, and compiles the final report from those files.

The goal is adversarial: every sub-agent should actively attempt to break the system by probing validation boundaries, injecting malicious payloads, triggering race conditions, and exhausting resource limits. Passing all numbered cases is the minimum; discovering new defects through creative edge-case exploration is the real objective.

---

## Test Environment

The NeuronPrompter server must be running in `web` mode before tests begin. The `web` mode exposes both the core API endpoints and the `/api/v1/web/*` management endpoints required by sections 1 and 22. The MCP server is tested separately via stdio.

- **Server:** `neuronprompter web --port 3030 --bind 127.0.0.1`
- **Base URL:** `http://127.0.0.1:3030`
- **API prefix:** `/api/v1`
- **Output directory:** Create a subfolder `neuronprompter_tests/` in the working directory for all test output and result files.
- **Ollama:** Optional. If Ollama is not running locally, SA-OLLAMA marks all Ollama-dependent cases as SKIP.
- **MCP:** For MCP tests, launch `neuronprompter mcp serve` as a subprocess communicating via stdio JSON-RPC 2.0.

---

## Orchestration Protocol

### Phase 0 - Bootstrap (Main Agent Executes Directly)

The main agent runs these steps sequentially. No sub-agents are used in this phase.

1. **System checks (Section 1):** Execute cases 1.1-1.5. Record health status, version, setup state, doctor probes.
2. **Create test_user_A:** `POST /api/v1/users` with `{"username": "test_user_a", "display_name": "Test User A"}`. Record **user_A_id**.
3. **Create session for user_A:** `POST /api/v1/sessions` with `{"user_id": <user_A_id>}`. Store the `np_session` cookie from the `Set-Cookie` response header. All subsequent requests must include this cookie for authentication.
4. **Create test_user_B:** `POST /api/v1/users` with `{"username": "test_user_b", "display_name": "Test User B"}`. Record **user_B_id**. Do NOT switch to this user.
5. **Seed taxonomy for user_A:** Create 2 tags (`tag_alpha`, `tag_beta`), 2 categories (`cat_general`, `cat_technical`), 1 collection (`coll_favorites`). Record all IDs.
6. **Seed prompts for user_A:** Create 5 prompts with varying content, some with language codes, some with tag/category associations. At least one prompt must contain template variables `{{name}}` and `{{topic}}`. Record all prompt IDs.
7. **Seed scripts for user_A:** Create 3 scripts (`script_language`: "python", "bash", "javascript"). Record all script IDs.
8. **Seed chains for user_A:** Create 2 chains:
   - **chain_A:** Uses `prompt_ids` (legacy format) with 3 prompts, custom separator `"\n---\n"`.
   - **chain_B:** Uses mixed `steps` (2 prompts + 1 script) with default separator.
   Record both chain IDs.
9. **Seed data for user_B:** Switch to user_B (via `PUT /api/v1/sessions/switch`), create 1 prompt, 1 tag. Switch back to user_A.
10. **Indexing edge cases (Section 2 remainder):** Execute cases 2.1-2.8.
11. **Write bootstrap file:** Write `neuronprompter_tests/bootstrap.md` containing:
    - user_A_id, user_B_id
    - All entity IDs with names (prompts, scripts, chains, tags, categories, collections)
    - System info from section 1
    - Server base URL, output directory path
    - Results for sections 1 and 2 (using the result file template below)

### Phase 1 - Parallel Sub-Agents (Launch ALL Simultaneously)

After bootstrap completes, the main agent launches all sub-agents in a single message. Each sub-agent receives the bootstrap data and its assigned test sections. Each sub-agent writes its results to the specified file in `neuronprompter_tests/`.

| Sub-Agent | Sections | User(s) | Result File | Scope |
|-----------|----------|---------|-------------|-------|
| **SA-PROMPT** | 3, 4 | user_A (read-write) | `section_03_04_prompt.md` | Prompt CRUD, versioning, lifecycle |
| **SA-SCRIPT** | 5, 6 | user_A (read-write) | `section_05_06_script.md` | Script CRUD, versioning, sync, import |
| **SA-CHAIN** | 7, 8, 9 | user_A (read-only on bootstrap chains) | `section_07_08_09_chain.md` | Chain CRUD, templates, composition |
| **SA-TAXONOMY** | 10 | user_A (read-write) | `section_10_taxonomy.md` | Tags, categories, collections, associations |
| **SA-SEARCH** | 11 | user_A (read-only) | `section_11_search.md` | Full-text search, filters, pagination |
| **SA-USER** | 12, 13 | creates own users | `section_12_13_user.md` | User CRUD, settings, isolation, SSRF, copy-to-user (prompts, scripts, chains), bulk copy |
| **SA-IO** | 14 | user_A (read-only for export) | `section_14_io.md` | Import, export, backup, path traversal |
| **SA-OLLAMA** | 15 | user_A | `section_15_ollama.md` | Ollama improve, translate, autofill, SSRF |
| **SA-MCP** | 16, 17 | mcp_agent (auto-created) | `section_16_17_mcp.md` | MCP tools, isolation, access boundaries |
| **SA-VERSION** | 18 | user_A (creates own prompts/scripts) | `section_18_version.md` | Version history, restore, consistency |
| **SA-STRESS** | 19, 20 | creates own user | `section_19_20_stress.md` | Boundary limits, security, injection |
| **SA-FEATURES** | 23 | user_A (read-write) | `section_23_features.md` | Bulk ops, chain variables, version compare, settings PATCH, export filters, search consistency |
| **SA-SESSION** | 24 | creates own sessions | `section_24_session.md` | Session create, switch, inspect, logout, cookie handling |
| **SA-COUNTS** | 25 | user_A (read-only) | `section_25_counts.md` | Count and languages endpoints for prompts, scripts, chains |

### Phase 2 - End-to-End (After Phase 1 Completes)

The main agent waits for all Phase 1 sub-agents to finish. Then:

| Sub-Agent | Sections | Notes |
|-----------|----------|-------|
| **SA-E2E** | 21 | Full cross-component workflows. Creates own users/data as needed. |
| **SA-WEB** | 22 | Web-specific endpoints (setup, doctor, SSE, Ollama mgmt, dialogs). |

### Phase 3 - Final Report (Main Agent Compiles)

1. Read `neuronprompter_tests/bootstrap.md`
2. Read ALL files matching `neuronprompter_tests/section_*.md`
3. Compile `FEHLERBERICHT.md` in the output directory with this structure:
   - **Test Environment** -- OS, NeuronPrompter version, Rust version, Ollama status, server bind address, database path
   - **Regression Case Results** -- one row per REF number: REF-ID, case number, status (PASS/FAIL/SKIP), observed behavior if different from expected
   - **Newly Discovered Defects** -- for each defect: unique ID, severity (critical/major/minor), endpoint/tool name, reproduction steps, expected behavior, actual behavior, whether it is a regression or new finding
   - **Correctly Functioning Features** -- table: feature | cases tested | status | remarks
   - **Test Statistics** -- total API calls made, total MCP tool calls made, total cases executed, defects found by severity, cases that produced unexpected behavior
   - **MCP Isolation Assessment** -- summary of whether the mcp_agent boundary holds (no cross-user data leakage, no delete operations, no Ollama access)
   - **Validation Boundary Assessment** -- summary of all boundary-value tests (field limits, SSRF, path traversal, injection attempts)
   - **Security Assessment** -- summary of SQL injection, XSS, CRLF injection, path traversal, SSRF bypass attempt results

---

## Sub-Agent Result File Template

Every sub-agent writes exactly this format to its result file. No deviation.

```markdown
# <Sub-Agent Name>: Sections X, Y, Z
**Timestamp:** <ISO 8601 datetime>
**Users used:** user_A=<id>, user_B=<id>
**Server:** <base_url>

## Results
| # | Case | Result | Deviation |
|---|------|--------|-----------|
| X.1 | brief description | PASS | |
| X.2 | brief description | FAIL | actual: "error 404" expected: "session not found" |
| X.3 | brief description | SKIP | Ollama not available |

## Edge Cases Discovered
| # | Description | Result | Deviation |
|---|-------------|--------|-----------|
| X.E1 | description of discovered edge case | PASS | |

## Defects
| ID | Severity | Endpoint/Tool | Description | Repro |
|----|----------|---------------|-------------|-------|
| DEF-001 | major | POST /api/v1/prompts | empty title accepted | POST {"title":"","content":"x"} |

## State for Downstream
entity_ids_created: []
entity_ids_deleted: []
notes: ""
```

---

## Sub-Agent Prompt Template

The main agent sends each sub-agent a prompt structured as follows. The `TEST CASES` section contains the exact tables from the relevant sections of this document.

```
You are a test sub-agent for NeuronPrompter. Execute test sections [X, Y, Z] completely.

BOOTSTRAP DATA:
- user_A: <id> (username: test_user_a) -- DO NOT DELETE
- user_B: <id> (username: test_user_b) -- DO NOT DELETE
- session_cookie: "np_session=<token>" (include in Cookie header for all authenticated requests)
- prompt_ids: {<id1>: "title1", <id2>: "title2", ...}
- script_ids: {<id1>: "title1", ...}
- chain_ids: {<id1>: "title1", <id2>: "title2"}
- tag_ids: {<id1>: "tag_alpha", <id2>: "tag_beta"}
- category_ids: {<id1>: "cat_general", <id2>: "cat_technical"}
- collection_ids: {<id1>: "coll_favorites"}
- Server base URL: <url>
- Output directory: <path>
- System info: version=<v>, Ollama=<status>

TEST CASES:
[paste the relevant section tables from TESTPLAN.md here]

RULES:
1. Execute every numbered case listed above.
2. After the numbered cases, discover and test edge cases (ID format: X.E1, X.E2, ...).
   Be adversarial: try to crash the server, leak data across users, bypass validation,
   trigger panics, exhaust memory, and find any behavior that deviates from expectations.
3. Write results to: neuronprompter_tests/<assigned_filename>.md using the result file template.
4. For each FAIL: record exact actual vs expected behavior in the Deviation column.
5. For each defect: assign severity (critical/major/minor) and provide exact reproduction steps.
6. DO NOT delete user_A or user_B. Create your own entities for destructive tests.
7. The result .md file is your ONLY deliverable. Everything relevant goes into it.
8. Use curl, httpie, or equivalent HTTP calls for REST API tests.
9. For MCP tests, use the stdio JSON-RPC 2.0 protocol.
```

---

## Conflict Notes

Read these before launching sub-agents. They document known interactions between concurrent sub-agents.

- **SA-PROMPT creates/deletes prompts on user_A:** SA-CHAIN reads bootstrap prompts from user_A. SA-PROMPT must NOT delete any of the 5 bootstrap prompts. It should create its own prompts for delete/lifecycle tests.
- **SA-SCRIPT creates/deletes scripts on user_A:** SA-CHAIN reads bootstrap scripts. SA-SCRIPT must NOT delete any of the 3 bootstrap scripts. Create new scripts for destructive tests.
- **SA-CHAIN reads bootstrap chains:** SA-CHAIN may create new chains but must NOT delete chain_A or chain_B used by other sub-agents for reference.
- **SA-USER creates its own test_user_C for cascade delete tests:** NEVER delete test_user_A or test_user_B. The cascade delete test (12.9) requires a dedicated user.
- **SA-MCP operates as mcp_agent:** This user is auto-created by the MCP server on first run. It is completely isolated from test_user_A and test_user_B. No conflicts possible.
- **SA-STRESS creates its own stress_user:** All stress tests operate on a dedicated user to avoid polluting other sub-agents' data.
- **SA-IO writes to neuronprompter_tests/io/:** Each I/O test uses unique filenames to avoid collisions with other sub-agents.
- **SA-OLLAMA requires a running Ollama instance:** If Ollama is unavailable, all Ollama-specific cases are marked SKIP. This does not affect other sub-agents.
- **SA-TAXONOMY modifies tags/categories/collections on user_A:** SA-SEARCH reads these associations. SA-TAXONOMY should create its own taxonomy entities for delete tests, not delete the bootstrap ones.
- **SA-VERSION creates its own prompts/scripts:** No conflicts with other sub-agents since it operates on newly created entities only.
- **Rate limiting (SA-STRESS section 19.19):** The rate limiter is per-IP. SA-STRESS should execute the rate-limit test last in its sequence to avoid impacting concurrent sub-agents sharing the same IP. Alternatively, wait 60 seconds before other sub-agents start.
- **SA-SESSION creates/destroys sessions:** Session tests operate on isolated session tokens (separate cookie jars). SA-SESSION must NEVER invalidate sessions created by the bootstrap phase or other sub-agents. Use freshly created sessions for all destructive tests (logout, expiry).
- **SA-COUNTS reads entity counts:** Counts depend on stable data. SA-COUNTS should create a dedicated user for count verification to avoid interference from concurrent sub-agents creating/deleting entities on user_A.
- **SA-FEATURES modifies prompts/scripts/chains on user_A:** The bulk-update tests (23.1-23.9) modify archive/favorite state. SA-FEATURES must create its own entities for bulk operations to avoid conflicting with SA-PROMPT, SA-SCRIPT, and SA-CHAIN.
- **SA-USER now includes copy-to-user for scripts and chains (12.21-12.28):** Copy operations create entities on user_B. SA-MCP is isolated (mcp_agent). SA-TAXONOMY and SA-SEARCH may observe copied data if they query user_B's associations.

---

## 1. System Infrastructure Verification

**Sub-Agent:** BOOTSTRAP (Phase 0, main agent)

| # | Case | Expected |
|---|------|----------|
| 1.1 | `GET /api/v1/health` | Returns 200 with status information |
| 1.2 | `neuronprompter version` (CLI) | Returns version string matching Cargo.toml (0.1.0) |
| 1.3 | `GET /api/v1/web/setup/status` | Returns `is_first_run` boolean and `data_dir` path |
| 1.4 | `GET /api/v1/web/doctor/probes` | Returns array of dependency probes with `name`, `available`, `required` fields |
| 1.5 | `GET /api/v1/web/mcp/status` | Returns MCP registration status for available targets |

---

## 2. Bootstrap & Data Setup

**Sub-Agent:** BOOTSTRAP (Phase 0, main agent)

| # | Case | Expected |
|---|------|----------|
| 2.1 | `POST /api/v1/users` with `{"username":"test_user_a","display_name":"Test User A"}` | 201, user created with valid ID |
| 2.2 | `POST /api/v1/users` with `{"username":"test_user_a","display_name":"Duplicate"}` | 409 DUPLICATE -- username uniqueness enforced |
| 2.3 | `PUT /api/v1/users/{user_A_id}/switch` | 200, active user switched |
| 2.4 | `POST /api/v1/tags` with `{"name":"tag_alpha","user_id":<user_A_id>}` | 201, tag created |
| 2.5 | `POST /api/v1/prompts` with title, content, tag_ids, category_ids | 201, prompt created with associations |
| 2.6 | `POST /api/v1/scripts` with title, content, script_language="python" | 201, script created |
| 2.7 | `POST /api/v1/chains` with prompt_ids and separator | 201, chain created |
| 2.8 | `POST /api/v1/chains` with mixed steps `[{"step_type":"prompt","item_id":<id>},{"step_type":"script","item_id":<id>}]` | 201, mixed chain created |

---

## 3. Prompt CRUD & Lifecycle

**Sub-Agent:** SA-PROMPT

| # | Case | Expected |
|---|------|----------|
| 3.1 | `POST /api/v1/prompts` with title and content only | 201, prompt created with auto-generated ID, timestamps set |
| 3.2 | `POST /api/v1/prompts` with all fields (title, content, description, notes, language="en", tag_ids, category_ids, collection_ids) | 200, all fields persisted; GET returns all associations |
| 3.3 | `GET /api/v1/prompts/{id}` for existing prompt | 200, full prompt with tags, categories, collections arrays |
| 3.4 | `GET /api/v1/prompts/99999` (non-existent) | 404 NOT_FOUND |
| 3.5 | `GET /api/v1/prompts/{id}` for a prompt owned by user_B while active user is user_A | 404 NOT_FOUND (user isolation) |
| 3.6 | `PUT /api/v1/prompts/{id}` with new title only | 200, title updated; content unchanged |
| 3.7 | `PUT /api/v1/prompts/{id}` with new content only | 200, content updated; title unchanged |
| 3.8 | `PUT /api/v1/prompts/{id}` with `description: null` to clear | 200, description cleared (set to null) |
| 3.9 | `PUT /api/v1/prompts/{id}` with `notes: null` to clear | 200, notes cleared |
| 3.10 | `PUT /api/v1/prompts/{id}` with `language: null` to clear | 200, language cleared |
| 3.11 | `PUT /api/v1/prompts/{id}` with new tag_ids (replaces all) | 200, old tags removed, new tags set |
| 3.12 | `PUT /api/v1/prompts/{id}` with `tag_ids: []` (remove all tags) | 200, no tags associated |
| 3.13 | `DELETE /api/v1/prompts/{id}` for a prompt NOT in any chain | 200, prompt deleted; GET returns 404 |
| 3.14 | `DELETE /api/v1/prompts/{id}` for a prompt that IS in a chain | 409 PROMPT_IN_USE with `chain_titles` listing referencing chains |
| 3.15 | `DELETE /api/v1/prompts/99999` | 404 NOT_FOUND |
| 3.16 | `POST /api/v1/prompts/{id}/duplicate` | 200, new prompt created with title suffixed "(copy)"; all associations copied |
| 3.17 | Verify duplicated prompt has different ID but identical content, description, notes, language, tags, categories, collections | All fields match except ID and title |
| 3.18 | `PATCH /api/v1/prompts/{id}/favorite` (toggle on) | 200, `is_favorite: true` |
| 3.19 | `PATCH /api/v1/prompts/{id}/favorite` again (toggle off) | 200, `is_favorite: false` |
| 3.20 | `PATCH /api/v1/prompts/{id}/archive` (toggle on) | 200, `is_archived: true` |
| 3.21 | `PATCH /api/v1/prompts/{id}/archive` again (toggle off) | 200, `is_archived: false` |
| 3.22 | `POST /api/v1/prompts` with `title: ""` (empty) | 400 VALIDATION_ERROR: "Title must not be empty" |
| 3.23 | `POST /api/v1/prompts` with title of exactly 201 characters | 400 VALIDATION_ERROR: "Title must not exceed 200 characters" |
| 3.24 | `POST /api/v1/prompts` with title of exactly 200 characters | 200, accepted |
| 3.25 | `POST /api/v1/prompts` with `content: ""` (empty) | 400 VALIDATION_ERROR: "Content must not be empty" |
| 3.26 | `POST /api/v1/prompts` with content of exactly 1,048,576 bytes (1 MB) | 200, accepted |
| 3.27 | `POST /api/v1/prompts` with content of 1,048,577 bytes (1 MB + 1 byte) | 400 VALIDATION_ERROR: content exceeds maximum size |
| 3.28 | `POST /api/v1/prompts` with `language: "xyz"` (3 chars, not ISO 639-1) | 400 VALIDATION_ERROR: "Language code must be exactly 2 lowercase letters" |
| 3.29 | `POST /api/v1/prompts` with `language: "EN"` (uppercase) | 400 VALIDATION_ERROR |
| 3.30 | `POST /api/v1/prompts` with `language: "e"` (1 char) | 400 VALIDATION_ERROR |
| 3.31 | `POST /api/v1/prompts` with `language: "en"` | 200, accepted |
| 3.32 | `POST /api/v1/prompts` with `language: "12"` (digits) | 400 VALIDATION_ERROR |

---

## 4. Prompt Versioning

**Sub-Agent:** SA-PROMPT

| # | Case | Expected |
|---|------|----------|
| 4.1 | Create prompt -> update title -> `GET /api/v1/versions/prompt/{id}` | Version list has 1 entry with original title and content |
| 4.2 | Update prompt content -> check version list | Version list has 2 entries; latest version has pre-update content |
| 4.3 | Verify `version_number` increments: first version=1, second=2 | Monotonically increasing |
| 4.4 | Update prompt with identical title and content (no actual change) | No new version created; version count unchanged |
| 4.5 | `GET /api/v1/versions/prompt/{id}` for all versions | Returns list sorted by version_number |
| 4.6 | `GET /api/v1/versions/{version_id}` for specific version | Returns version with correct content snapshot |
| 4.7 | `POST /api/v1/versions/prompt/{id}/restore` with `{"version_id": <old_version_id>}` | Prompt content restored to old version; new version created as snapshot of pre-restore state |
| 4.8 | `GET /api/v1/versions/prompt/99999` (non-existent prompt) | 404 NOT_FOUND |
| 4.9 | `GET /api/v1/versions/prompt/{id}` for newly created prompt (never edited) | Empty version list (no edits = no snapshots) |
| 4.10 | Update only `description` (not title or content) -> check versions | New version created capturing pre-edit state |
| 4.11 | Update only `notes` -> check versions | New version created |
| 4.12 | Update only `language` -> check versions | New version created |
| 4.13 | Perform 5 sequential edits -> verify 5 versions exist with correct ordering | version_number 1 through 5, each with correct snapshot |
| 4.14 | Restore to version 2, then check current prompt state | Content matches version 2's snapshot; version 6 created |
| 4.15 | `POST /api/v1/versions/prompt/{id}/restore` with non-existent version_id | 404 NOT_FOUND |

---

## 5. Script CRUD & Lifecycle

**Sub-Agent:** SA-SCRIPT

| # | Case | Expected |
|---|------|----------|
| 5.1 | `POST /api/v1/scripts` with title, content, script_language="python" | 201, script created |
| 5.2 | `POST /api/v1/scripts` with all fields (title, content, script_language, description, notes, language, tag_ids, category_ids, collection_ids) | 200, all fields persisted |
| 5.3 | `POST /api/v1/scripts` without script_language | 400 VALIDATION_ERROR: script_language required |
| 5.4 | `POST /api/v1/scripts` with `script_language: ""` (empty) | 400 VALIDATION_ERROR |
| 5.5 | `POST /api/v1/scripts` with `script_language: "Python"` (uppercase) | 400 VALIDATION_ERROR: "must contain only lowercase alphanumeric characters and hyphens" |
| 5.6 | `POST /api/v1/scripts` with `script_language: "c++"` (special char) | 400 VALIDATION_ERROR |
| 5.7 | `POST /api/v1/scripts` with `script_language: "python-3"` (with hyphen) | 200, accepted |
| 5.8 | `POST /api/v1/scripts` with script_language of exactly 30 characters | 200, accepted |
| 5.9 | `POST /api/v1/scripts` with script_language of 31 characters | 400 VALIDATION_ERROR |
| 5.10 | `GET /api/v1/scripts/{id}` | 200, full script with associations |
| 5.11 | `GET /api/v1/scripts/99999` | 404 NOT_FOUND |
| 5.12 | `PUT /api/v1/scripts/{id}` with new content | 200, content updated |
| 5.13 | `PUT /api/v1/scripts/{id}` with new script_language | 200, language updated |
| 5.14 | `DELETE /api/v1/scripts/{id}` for script NOT in any chain | 200, deleted |
| 5.15 | `DELETE /api/v1/scripts/{id}` for script that IS in a chain | 409 SCRIPT_IN_USE with chain_titles |
| 5.16 | `POST /api/v1/scripts/{id}/duplicate` | 200, new script with "(copy)" suffix |
| 5.17 | `PATCH /api/v1/scripts/{id}/favorite` | 200, toggled |
| 5.18 | `PATCH /api/v1/scripts/{id}/archive` | 200, toggled |
| 5.19 | Validation: empty title -> 400 | Same rules as prompts |
| 5.20 | Validation: title >200 chars -> 400 | Same rules as prompts |
| 5.21 | Validation: empty content -> 400 | Same rules as prompts |
| 5.22 | Validation: content >1MB -> 400 | Same rules as prompts |
| 5.23 | `POST /api/v1/scripts` with `script_language: "c_sharp"` (underscore) | 400 VALIDATION_ERROR (only lowercase, digits, hyphens) |
| 5.24 | `POST /api/v1/scripts` with `script_language: "123"` (digits only) | 200, accepted |
| 5.25 | GET script owned by user_B while active user is user_A | 404 NOT_FOUND (isolation) |

---

## 6. Script Sync & Import

**Sub-Agent:** SA-SCRIPT

| # | Case | Expected |
|---|------|----------|
| 6.1 | Create a temp directory with 3 script files (.py, .sh, .js) -> `POST /api/v1/scripts/sync` with directory path | 200, 3 scripts imported; languages auto-detected ("python", "bash", "javascript") |
| 6.2 | Run sync again on same directory (no changes) | 200, no new scripts imported (idempotent) |
| 6.3 | Add a new .rb file to directory -> sync again | 200, 1 new script imported with language="ruby" |
| 6.4 | `POST /api/v1/scripts/import-file` with a .py file | 201, script created with language auto-detected |
| 6.5 | `POST /api/v1/scripts/import-file` with explicit script_language override | 200, uses provided language, not auto-detected |
| 6.6 | Sync with non-existent directory path | Error: directory not found |
| 6.7 | Sync with empty directory (no script files) | 200, 0 scripts imported |
| 6.8 | Import file without extension | 200 or error depending on auto-detection behavior |
| 6.9 | Import binary file (e.g., a .png) | Error or graceful handling (no crash) |
| 6.10 | Import file with path traversal attempt (`../../etc/passwd`) | 403 PATH_TRAVERSAL |
| 6.11 | Sync directory containing subdirectories | Behavior documented: recursive or flat? |
| 6.12 | Import file >1MB | 400 VALIDATION_ERROR: content exceeds max size |
| 6.13 | After sync, verify synced scripts have `source_path` set and `is_synced: true` | `GET /api/v1/scripts/{id}` returns both fields populated correctly |
| 6.14 | Modify a synced file on disk, then re-sync | Script content updated to match new file content; `source_path` unchanged |
| 6.15 | Two users sync the same directory | Each user gets their own independent copy of scripts; no cross-user contamination |

---

## 7. Chain CRUD & Composition

**Sub-Agent:** SA-CHAIN

| # | Case | Expected |
|---|------|----------|
| 7.1 | `POST /api/v1/chains` with `prompt_ids=[p1, p2, p3]` and separator `"\n---\n"` | 201, chain created with 3 steps |
| 7.2 | `POST /api/v1/chains` with mixed steps `[{"step_type":"prompt","item_id":p1},{"step_type":"script","item_id":s1}]` | 200, chain with mixed prompt+script steps |
| 7.3 | `POST /api/v1/chains` with `prompt_ids: []` (empty, legacy format) | 400 VALIDATION_ERROR: "A chain must contain at least one prompt" |
| 7.3b | `POST /api/v1/chains` with `steps: []` (empty, mixed-steps format) | 400 VALIDATION_ERROR: "A chain must contain at least one step" |
| 7.4 | `POST /api/v1/chains` with steps `[{"step_type":"invalid","item_id":1}]` | 400 VALIDATION_ERROR: "Invalid step type" |
| 7.5 | `GET /api/v1/chains/{id}` | 200, chain with resolved steps (each step includes full prompt/script data) |
| 7.6 | `GET /api/v1/chains/{id}/content` | 200, composed content (all step contents joined by separator) |
| 7.7 | Verify composition: create chain with 2 prompts, separator `" | "` -> GET content | Content equals `prompt1_content + " | " + prompt2_content` |
| 7.8 | Chain with single step -> GET content | Content equals single prompt content (no separator) |
| 7.9 | `PUT /api/v1/chains/{id}` with new `prompt_ids` order (reversed) | 200, step order updated; GET content reflects new order |
| 7.10 | `PUT /api/v1/chains/{id}` with new separator | 200, separator updated; GET content uses new separator |
| 7.11 | `PUT /api/v1/chains/{id}` with `title: "Updated Chain"` | 200, title changed |
| 7.12 | `PUT /api/v1/chains/{id}` with `description: null` | 200, description cleared |
| 7.13 | `DELETE /api/v1/chains/{id}` | 200, chain deleted; prompts/scripts still exist |
| 7.14 | `POST /api/v1/chains/{id}/duplicate` | 200, new chain with "(copy)" suffix; steps copied |
| 7.15 | `PATCH /api/v1/chains/{id}/favorite` | 200, toggled |
| 7.16 | `PATCH /api/v1/chains/{id}/archive` | 200, toggled |
| 7.17 | `GET /api/v1/chains/by-prompt/{prompt_id}` | 200, list of chains containing this prompt |
| 7.18 | `GET /api/v1/chains/by-prompt/99999` | 200, empty list |
| 7.19 | Create chain with `prompt_ids=[99999]` (non-existent prompt) | Error: prompt not found |
| 7.20 | Create chain with steps referencing non-existent script | Error: script not found |
| 7.21 | `POST /api/v1/chains` with separator of exactly 100 characters | 200, accepted |
| 7.22 | `POST /api/v1/chains` with separator of 101 characters | 400 VALIDATION_ERROR: "Separator must not exceed 100 characters" |
| 7.23 | `POST /api/v1/chains` with `separator: ""` (empty) | 400 VALIDATION_ERROR: separator must not be empty |
| 7.24 | Mixed chain: delete the script referenced by a step -> GET chain content | Error or graceful degradation (documents behavior) |
| 7.25 | Chain with tags/categories/collections associations | 200, associations persisted and returned |

---

## 8. Chain Template Variables

**Sub-Agent:** SA-CHAIN

| # | Case | Expected |
|---|------|----------|
| 8.1 | Create prompt with content `"Hello {{name}}, topic: {{topic}}"` -> add to chain -> GET content | Content contains `{{name}}` and `{{topic}}` literally (no substitution on GET) |
| 8.2 | `POST /api/v1/clipboard/copy-substituted` with `{"content": "<chain content>", "variables": {"name": "Alice", "topic": "Rust"}}` | 200, substituted content "Hello Alice, topic: Rust" copied to clipboard. Note: `GET /api/v1/chains/{id}/content` is GET-only and does NOT accept a request body for substitution. |
| 8.3 | Substitution with missing variable: provide only `name`, omit `topic` | `{{topic}}` remains unsubstituted in output |
| 8.4 | Prompt with duplicate variables: `"{{x}} and {{x}} again"` -> substitute `x="val"` | Both replaced: "val and val again" |
| 8.5 | Variable at start: `"{{greeting}} world"` | Correctly replaced at position 0 |
| 8.6 | Variable at end: `"world {{end}}"` | Correctly replaced at end |
| 8.7 | Empty variable name: `"text {{}}"` | `{{}}` treated as literal text (not a variable) |
| 8.8 | Nested braces: `"{{a{{b}}}}"` | Parser handles gracefully; no crash |
| 8.9 | Whitespace in variable: `"{{ name }}"` | Behavior documented (trimmed or literal?) |
| 8.10 | Special characters in variable: `"{{name!}}"` | Not recognized as variable (invalid chars); left as literal |
| 8.11 | Unicode variable name: `"{{名前}}"` | Behavior documented (accepted or rejected?) |
| 8.12 | Variable starting with digit: `"{{1var}}"` | Not recognized as valid variable |
| 8.13 | Variable with underscore: `"{{_private}}"` | Recognized as valid variable |
| 8.14 | 100 unique variables in one prompt | All extracted and substitutable |
| 8.15 | Variable name matching: case sensitive `{{Name}}` vs `{{name}}` | Treated as different variables |

---

## 9. Chain Content with Template Substitution

**Sub-Agent:** SA-CHAIN

| # | Case | Expected |
|---|------|----------|
| 9.1 | `POST /api/v1/clipboard/copy-substituted` with chain content and variable map | 200, substituted content copied to clipboard |
| 9.2 | `GET /api/v1/chains/{id}/content` (no substitution) | 200, raw template content with `{{variables}}` intact |
| 9.3 | Mixed chain (prompt with `{{var}}` + script without variables) -> substitute | Only prompt portion gets substitution; script content unchanged |
| 9.4 | Chain with 3 steps and separator `"\n\n"` -> verify separator placement | Separator appears between steps, not before first or after last |
| 9.5 | Chain with 1 step -> GET content | No separator in output |
| 9.6 | Delete a prompt from chain's steps (via API), then GET chain content | Error: referenced prompt not found, or graceful degradation |
| 9.7 | Chain where all prompts have empty-string content (after validation bypass or update) | Content is just separators (documents behavior) |
| 9.8 | Substitute with HTML in variable value: `{"name": "<script>alert(1)</script>"}` | Value inserted literally (no sanitization needed -- content is raw text, not rendered HTML) |

---

## 10. Taxonomy - Tags, Categories, Collections

**Sub-Agent:** SA-TAXONOMY

| # | Case | Expected |
|---|------|----------|
| 10.1 | `POST /api/v1/tags` with `{"name":"new_tag","user_id":<user_A_id>}` | 201, tag created |
| 10.2 | `GET /api/v1/tags/user/{user_A_id}` | 200, list includes new_tag and bootstrap tags |
| 10.3 | `PUT /api/v1/tags/{id}` with `{"name":"renamed_tag"}` | 200, tag renamed |
| 10.4 | `DELETE /api/v1/tags/{id}` | 200, tag deleted |
| 10.5 | `POST /api/v1/tags` with duplicate name for same user | 409 DUPLICATE |
| 10.6 | `POST /api/v1/categories` with `{"name":"new_cat","user_id":<user_A_id>}` | 201, category created |
| 10.7 | `GET /api/v1/categories/user/{user_A_id}` | 200, list includes new_cat |
| 10.8 | `PUT /api/v1/categories/{id}` rename | 200, renamed |
| 10.9 | `DELETE /api/v1/categories/{id}` | 200, deleted |
| 10.10 | Duplicate category name -> 409 | 409 DUPLICATE |
| 10.11 | `POST /api/v1/collections` with `{"name":"new_coll","user_id":<user_A_id>}` | 201, collection created |
| 10.12 | `GET /api/v1/collections/user/{user_A_id}` | 200, list includes new_coll |
| 10.13 | `PUT /api/v1/collections/{id}` rename | 200, renamed |
| 10.14 | `DELETE /api/v1/collections/{id}` | 200, deleted |
| 10.15 | Taxonomy name of exactly 100 characters | 200, accepted |
| 10.16 | Taxonomy name of 101 characters | 400 VALIDATION_ERROR: "Name must not exceed 100 characters" |
| 10.17 | Taxonomy name of only whitespace `"   "` | 400 VALIDATION_ERROR (trimmed to empty) |
| 10.18 | Taxonomy name empty `""` | 400 VALIDATION_ERROR |
| 10.19 | Assign tag to prompt via `PUT /api/v1/prompts/{id}` with `tag_ids` | 200, tag associated |
| 10.20 | Verify prompt's tag list includes the assigned tag | GET prompt returns tag in associations |
| 10.21 | Assign same tag to script via `PUT /api/v1/scripts/{id}` with `tag_ids` | 200, tag associated with script |
| 10.22 | Assign category and collection to chain | 200, associations persisted |
| 10.23 | Delete tag that is associated with prompts -> verify prompt's tag list | Tag removed from prompt associations; prompt still exists |
| 10.24 | Delete category that is associated with scripts -> verify script | Category removed; script still exists |
| 10.25 | Taxonomy name with Unicode: `"中文标签"` | 200, accepted |
| 10.26 | Taxonomy name with SQL injection: `"'; DROP TABLE tags; --"` | 200, stored literally as name (no SQL execution) |
| 10.27 | `GET /api/v1/tags/user/{user_B_id}` while active user is user_A | Returns user_B's tags (or 403 depending on auth model) |

---

## 11. Search

**Sub-Agent:** SA-SEARCH

| # | Case | Expected |
|---|------|----------|
| 11.1 | `POST /api/v1/search/prompts` with `{"query":"<known prompt content substring>"}` | 200, matching prompts returned |
| 11.2 | `POST /api/v1/search/scripts` with query matching a script title | 200, matching scripts returned |
| 11.3 | `POST /api/v1/search/chains` with query matching a chain title | 200, matching chains returned |
| 11.4 | Search with `favorite: true` filter | Only favorited items returned |
| 11.5 | Search with `archived: true` filter | Only archived items returned |
| 11.6 | Search with `tag_id` filter | Only items with that tag returned |
| 11.7 | Search with `category_id` filter | Only items with that category returned |
| 11.8 | Search with `collection_id` filter | Only items in that collection returned |
| 11.9 | Search with `limit: 2` | At most 2 results returned |
| 11.10 | Search with `limit: 2, offset: 2` | Skips first 2 results |
| 11.11 | Search with `limit: 0` | Error or returns default limit |
| 11.12 | Search with `limit: 1001` | Capped to 1000 or error (max 1000) |
| 11.13 | Search with negative offset | Error or treated as 0 |
| 11.14 | Search with empty query `""` | Returns all results or error (documents behavior) |
| 11.15 | Search with combined filters: `favorite: true` + `tag_id` | Intersection of both filters |
| 11.16 | Search with query `"'; DROP TABLE prompts; --"` (SQL injection) | No crash; no data loss; empty or valid results |
| 11.17 | Search with Unicode query `"日本語のプロンプト"` | No crash; returns matching results if any |
| 11.18 | Search with special characters `"!@#$%^&*()_+-=[]{}\\|;':\",./<>?"` | No crash |
| 11.19 | Search with whitespace-only query `"   "` | Empty results or error |
| 11.20 | Search with very long query (10,000 characters) | No crash; timeout or truncation acceptable |
| 11.21 | Search returns results only for active user (not other users' data) | User isolation enforced |

---

## 12. User Management

**Sub-Agent:** SA-USER

**Warning:** Create test_user_C for destructive tests. NEVER delete test_user_A or test_user_B.

| # | Case | Expected |
|---|------|----------|
| 12.1 | `POST /api/v1/users` with valid username and display_name | 201, user created |
| 12.2 | Verify default UserSettings created for new user | `GET /api/v1/settings/user/{id}` returns defaults (theme, sort, etc.) |
| 12.3 | `POST /api/v1/users` with `username: "UPPERCASE"` | 400 VALIDATION_ERROR: lowercase only |
| 12.4 | `POST /api/v1/users` with `username: "user@name"` (special char) | 400 VALIDATION_ERROR: alphanumeric + underscore only |
| 12.5 | `POST /api/v1/users` with `username: ""` | 400 VALIDATION_ERROR: must not be empty |
| 12.6 | `POST /api/v1/users` with `username: "a b"` (space) | 400 VALIDATION_ERROR |
| 12.7 | `POST /api/v1/users` with `username: "valid_user_123"` | 200, accepted |
| 12.8 | `POST /api/v1/users` with `username: "___"` (only underscores) | 200, accepted |
| 12.9 | `PUT /api/v1/users/{user_C_id}/switch` | 200, active user changed to user_C |
| 12.10 | Verify `GET /api/v1/settings/app/last_user_id` reflects switch | Returns user_C_id |
| 12.11 | `DELETE /api/v1/users/{user_C_id}` (user_C has prompts, scripts, chains, tags) | 200, user and ALL dependent data deleted (CASCADE) |
| 12.12 | Verify cascade: `GET /api/v1/prompts/{user_C_prompt_id}` after user delete | 404 NOT_FOUND |
| 12.13 | Verify cascade: `GET /api/v1/tags/user/{user_C_id}` after user delete | Empty or 404 |
| 12.14 | `DELETE /api/v1/users/99999` | 403 FORBIDDEN (authorization check runs before existence check to prevent user enumeration) |
| 12.15 | **Multi-user isolation:** Create prompts on user_A, switch to user_B, `POST /api/v1/prompts/search` | Only user_B's prompts returned (NOT user_A's) |
| 12.16 | **Multi-user isolation:** Switch to user_A, search for user_B's prompt title | Not found |
| 12.17 | `PUT /api/v1/users/{user_A_id}` with new display_name | 200, display_name updated |
| 12.18 | `GET /api/v1/users` | 200, list of all users |
| 12.19 | **Copy prompt to user:** `POST /api/v1/prompts/{id}/copy-to-user` with `{"target_user_id": <user_B_id>}` | 200, prompt copied to target user with associations recreated |
| 12.20 | **Bulk copy:** `POST /api/v1/users/bulk-copy` from user_A to new user | 200, all prompts/scripts/chains copied |
| 12.21 | **Copy script to user:** `POST /api/v1/scripts/{id}/copy-to-user` with `{"target_user_id": <user_B_id>}` | 200, script copied to target user |
| 12.22 | Verify copied script belongs to target user | Switch to user_B, `GET /api/v1/scripts/{new_id}` returns the copied script |
| 12.23 | Copy script to non-existent user (99999) | 404 NOT_FOUND |
| 12.24 | **Copy chain to user:** `POST /api/v1/chains/{id}/copy-to-user` with `{"target_user_id": <user_B_id>}` | 200, deep copy: chain AND all referenced prompts/scripts are copied to target user |
| 12.25 | Verify copied chain steps reference newly created copies (not originals) | Switch to user_B, `GET /api/v1/chains/{new_chain_id}`, step item_ids point to copies owned by user_B |
| 12.26 | Verify chain copy includes taxonomy associations (tags, categories, collections) | Associations recreated for target user; missing taxonomy items auto-created |
| 12.27 | Copy chain with mixed steps (prompts + scripts) to another user | Both prompt and script steps are deep-copied to target user |
| 12.28 | Copy chain to same user (self-copy) | 200, works as duplicate (new chain with new step copies) or error (document behavior) |
| 12.29 | **Bulk copy depth:** `POST /api/v1/users/bulk-copy` -> verify all entity types copied | CopySummary counts match source user's prompt/script/chain/tag/category/collection counts |
| 12.30 | **Bulk copy step remapping:** Verify chain steps in target user reference newly-created copies | No step references source user's entity IDs |
| 12.31 | **Bulk copy ownership:** All copied entities have target_user_id | Query prompts/scripts/chains/tags for target user; all present |
| 12.32 | Bulk copy when source user has 0 items | 200, CopySummary with all zero counts |
| 12.33 | Bulk copy where source_user_id does not match authenticated session user | 403 FORBIDDEN |

---

## 13. User Settings & SSRF Protection

**Sub-Agent:** SA-USER

| # | Case | Expected |
|---|------|----------|
| 13.1 | `GET /api/v1/settings/user/{user_A_id}` | 200, returns theme, sort_field, sort_direction, ollama_base_url, ollama_model |
| 13.2 | `PUT /api/v1/settings/user` with `{"theme": "dark"}` | 200, theme updated |
| 13.3 | `PUT /api/v1/settings/user` with `{"sort_field": "title", "sort_direction": "asc"}` | 200, sort settings updated |
| 13.4 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://localhost:11434"` | 200, accepted (localhost allowed) |
| 13.5 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://127.0.0.1:11434"` | 200, accepted |
| 13.6 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://[::1]:11434"` | 200, accepted (IPv6 loopback) |
| 13.7 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://192.168.1.100:11434"` | 200, accepted (private network IPs allowed by design for LAN Ollama deployments) |
| 13.8 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://10.0.0.1:11434"` | 200, accepted (RFC 1918 private IPs allowed) |
| 13.9 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://evil.com:11434"` | 400 VALIDATION_ERROR: host not allowed |
| 13.10 | `PUT /api/v1/settings/user` with `ollama_base_url: "file:///etc/passwd"` | 400 VALIDATION_ERROR: unsupported scheme |
| 13.11 | `PUT /api/v1/settings/user` with `ollama_base_url: "ftp://localhost"` | 400 VALIDATION_ERROR: unsupported scheme |
| 13.12 | `PUT /api/v1/settings/user` with `ollama_base_url: "localhost:11434"` (no scheme) | 400 VALIDATION_ERROR: "URL must include a scheme" |
| 13.13 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://0.0.0.0:11434"` | 400 VALIDATION_ERROR: host not allowed |
| 13.14 | `PUT /api/v1/settings/user` with `ollama_base_url: "http://[::ffff:192.168.1.1]:11434"` (IPv6-mapped IPv4) | 400 VALIDATION_ERROR or accepted (document behavior) |
| 13.15 | `GET /api/v1/settings/app/last_user_id` | 200, returns current active user ID |
| 13.16 | `PUT /api/v1/settings/app/custom_key` with `{"value": "custom_value"}` | 403 FORBIDDEN: "app setting key 'custom_key' is not writable" (only `last_user_id` is in the allowlist) |
| 13.17 | `PUT /api/v1/settings/app/last_user_id` with `{"value": "<valid_user_id>"}` -> then `GET /api/v1/settings/app/last_user_id` | 200, setting stored and returned correctly; `last_user_id` validates that the user exists |
| 13.18 | `GET /api/v1/settings/app/nonexistent_key` | 404 or null value |

---

## 14. Import/Export

**Sub-Agent:** SA-IO

All file operations use the `neuronprompter_tests/io/` subdirectory.

| # | Case | Expected |
|---|------|----------|
| 14.1 | `POST /api/v1/io/export/json` with `{"path": "<output_dir>/export.json"}` | 200, JSON file created with user metadata, prompts, tags, categories, collections, version history |
| 14.2 | Inspect exported JSON structure | Contains `username`, `display_name`, `exported_at`, `prompts` array with all fields and associations |
| 14.3 | `POST /api/v1/io/import/json` with `{"path": "<output_dir>/export.json"}` on a different user | 200, prompts imported; missing tags/categories auto-created |
| 14.4 | Verify imported prompts match originals | Same title, content, description, notes, language; associations recreated |
| 14.5 | Import JSON file with version history | Versions imported and accessible via version API |
| 14.6 | `POST /api/v1/io/export/markdown` with `{"path": "<output_dir>/md_export/"}` | 200, directory created with .md files (YAML front-matter format) |
| 14.7 | Inspect markdown file structure | YAML front-matter: title, description, language, favorite, tags, categories, collections, notes; body: prompt content |
| 14.8 | `POST /api/v1/io/import/markdown` with markdown directory path | 200, prompts recreated from markdown files |
| 14.9 | `POST /api/v1/io/backup` with `{"path": "<output_dir>/backup.db"}` | 200, SQLite backup created (WAL-safe) |
| 14.10 | Import invalid JSON: `{"not_valid": true}` | 500 SERIALIZATION_ERROR or 400 |
| 14.11 | Import file with broken JSON syntax `{broken:` | 500 SERIALIZATION_ERROR |
| 14.12 | Import file larger than 50 MiB | Error: exceeds maximum import size |
| 14.13 | Export with path traversal: `"../../etc/dangerous"` | 403 PATH_TRAVERSAL |
| 14.14 | Import with path traversal: `"../../../etc/passwd"` | 403 PATH_TRAVERSAL |
| 14.15 | Export path containing null byte: `"export\x00.json"` | Error: invalid path |
| 14.16 | Export to non-existent parent directory | Error or directory auto-created (document behavior) |
| 14.17 | Import empty JSON file | Error: empty or invalid content |
| 14.18 | Import JSON with prompts having duplicate titles as existing prompts | Imported with deduplication or error (document behavior) |
| 14.19 | Export when user has 0 prompts | 200, valid JSON with empty prompts array |
| 14.20 | Round-trip: export JSON -> delete all prompts -> import JSON -> verify all restored | All prompts, associations, and versions restored correctly |

---

## 15. Ollama Integration

**Sub-Agent:** SA-OLLAMA

**Note:** If Ollama is not running locally, mark all happy-path cases as SKIP. Still test error handling paths.

| # | Case | Expected |
|---|------|----------|
| 15.1 | `POST /api/v1/ollama/status` with `{"base_url": "http://localhost:11434"}` | 200, `{"connected": true/false}` depending on Ollama availability. Note: `base_url` is a required field. |
| 15.2 | `POST /api/v1/ollama/improve` with valid prompt content and model | 200, improved prompt text returned |
| 15.3 | `POST /api/v1/ollama/translate` with valid prompt content, source/target language, model | 200, translated text returned |
| 15.4 | `POST /api/v1/ollama/autofill` with prompt content | 200, returns `DerivedMetadata` with fields: `description` (string), `language` (string, ISO 639-1), `notes` (string), `suggested_tags` (string array), `suggested_categories` (string array), `errors` (string array for partial failures) |
| 15.5 | `POST /api/v1/ollama/improve` when Ollama is not running | 502 OLLAMA_UNAVAILABLE |
| 15.6 | `POST /api/v1/ollama/translate` with non-existent model name | 502 OLLAMA_ERROR |
| 15.7 | `POST /api/v1/ollama/improve` with empty content | 400 VALIDATION_ERROR |
| 15.8 | `POST /api/v1/ollama/improve` with 1MB content | Behavior documented: timeout, truncation, or success |
| 15.9 | SSRF attempt via user settings: set ollama_base_url to `http://evil.com:11434` then call improve | 400 VALIDATION_ERROR at settings level (URL rejected before request sent) |
| 15.10 | SSRF attempt: `http://169.254.169.254` (cloud metadata) | 400 VALIDATION_ERROR: host not allowed |
| 15.11 | SSRF attempt: `http://[::ffff:127.0.0.1]:11434` (IPv6-mapped) | Document behavior: accepted or rejected? |
| 15.12 | SSRF attempt: `http://localhost.evil.com:11434` | 400 VALIDATION_ERROR: host not allowed |
| 15.13 | SSRF attempt: `gopher://localhost:11434` | 400 VALIDATION_ERROR: unsupported scheme |
| 15.14 | Multiple concurrent improve requests | No crash; requests serialized or parallelized (document behavior) |
| 15.15 | `POST /api/v1/ollama/improve` with prompt containing prompt injection attempt | Passes through to Ollama (NeuronPrompter does not filter LLM input) |

---

## 16. MCP Tools - Prompts & Scripts

**Sub-Agent:** SA-MCP

All MCP tests use the stdio JSON-RPC 2.0 protocol. The MCP server auto-creates an `mcp_agent` user on first run.

| # | Case | Expected |
|---|------|----------|
| 16.1 | `mcp_create_prompt(title="MCP Prompt", content="Test content")` | Success, returns prompt with ID |
| 16.2 | `mcp_create_prompt(title="Full Prompt", content="Content", description="Desc", notes="Notes", language="de", tag_ids=[], category_ids=[], collection_ids=[])` | Success, all fields set |
| 16.3 | `mcp_list_prompts()` | Returns list containing MCP Prompt; does NOT contain test_user_A's prompts |
| 16.4 | `mcp_get_prompt(prompt_id=<created_id>)` | Returns full prompt with associations |
| 16.5 | `mcp_get_prompt(prompt_id=99999)` | JSON-RPC error -32602 INVALID_PARAMS: "not found" |
| 16.6 | `mcp_update_prompt(prompt_id=<id>, title="Updated Title")` | Success, title changed |
| 16.7 | `mcp_update_prompt(prompt_id=<id>, description=null)` to clear | Success, description cleared |
| 16.8 | `mcp_update_prompt(prompt_id=<id>, tag_ids=[<tag_id>])` | Success, tags replaced |
| 16.9 | `mcp_search_prompts(query="MCP")` | Returns matching prompts |
| 16.10 | `mcp_search_prompts(query="test_user_a_prompt")` | Empty results (mcp_agent isolation) |
| 16.11 | `mcp_create_prompt(title="", content="x")` | JSON-RPC error -32602: title validation |
| 16.12 | `mcp_create_prompt(title="x", content="")` | JSON-RPC error -32602: content validation |
| 16.13 | `mcp_create_script(title="MCP Script", content="print('hello')", script_language="python")` | Success |
| 16.14 | `mcp_create_script(title="Bad", content="x", script_language="")` | JSON-RPC error -32602: script_language validation |
| 16.15 | `mcp_list_scripts()` | Returns MCP scripts only |
| 16.16 | `mcp_get_script(script_id=<id>)` | Success with full data |
| 16.17 | `mcp_update_script(script_id=<id>, content="updated")` | Success |
| 16.18 | `mcp_search_scripts(query="MCP Script")` | Returns matching scripts |
| 16.19 | Verify NO delete tool exists: attempt to call `mcp_delete_prompt` | JSON-RPC error: method not found |
| 16.20 | Verify NO Ollama tools exist: attempt to call `mcp_ollama_improve` | JSON-RPC error: method not found |
| 16.21 | `mcp_create_prompt` with title >200 chars | JSON-RPC error -32602 |
| 16.22 | `mcp_create_prompt` with content >1MB | JSON-RPC error -32602 |
| 16.23 | `mcp_create_tag(name="mcp_tag")` | Success |
| 16.24 | `mcp_create_tag(name="mcp_tag")` again (duplicate) | JSON-RPC error -32602: DUPLICATE |
| 16.25 | `mcp_list_tags()` | Returns tags owned by mcp_agent only |

---

## 17. MCP Tools - Chains & Taxonomy

**Sub-Agent:** SA-MCP

| # | Case | Expected |
|---|------|----------|
| 17.1 | `mcp_create_chain(title="MCP Chain", prompt_ids=[<mcp_prompt_id>])` | Success, chain created |
| 17.2 | `mcp_create_chain(title="With Sep", prompt_ids=[<id1>,<id2>], separator=" -- ")` | Success, custom separator |
| 17.3 | `mcp_create_chain(title="Empty", prompt_ids=[])` | JSON-RPC error -32602: at least one prompt required |
| 17.4 | `mcp_list_chains()` | Returns MCP chains only |
| 17.5 | `mcp_get_chain(chain_id=<id>)` | Returns chain with resolved steps |
| 17.6 | `mcp_get_chain_content(chain_id=<id>)` | Returns composed content |
| 17.7 | `mcp_update_chain(chain_id=<id>, title="Renamed Chain")` | Success |
| 17.8 | `mcp_update_chain(chain_id=<id>, prompt_ids=[<id2>,<id1>])` (reorder) | Success, order changed |
| 17.9 | `mcp_update_chain(chain_id=<id>, separator="|||")` | Success, separator updated |
| 17.10 | `mcp_duplicate_chain(chain_id=<id>)` | Success, new chain with "(copy)" suffix |
| 17.11 | `mcp_chains_for_prompt(prompt_id=<mcp_prompt_id>)` | Returns chains referencing this prompt |
| 17.12 | `mcp_chains_for_prompt(prompt_id=99999)` | Empty list |
| 17.13 | `mcp_search_chains(query="MCP Chain")` | Returns matching chains |
| 17.14 | `mcp_list_categories()` | Returns categories for mcp_agent |
| 17.15 | `mcp_create_category(name="mcp_category")` | Success |
| 17.16 | `mcp_list_collections()` | Returns collections for mcp_agent |
| 17.17 | `mcp_create_chain` with prompt_id from test_user_A | JSON-RPC error -32602: prompt not found (isolation) |
| 17.18 | `mcp_get_chain(chain_id=<test_user_A_chain_id>)` | JSON-RPC error: not found (isolation) |
| 17.19 | Verify JSON-RPC error codes: CoreError::Validation -> -32602 | Correct mapping |
| 17.20 | Verify JSON-RPC error codes: CoreError::NotFound -> -32602 | Correct mapping |
| 17.21 | Verify JSON-RPC error codes: CoreError::Duplicate -> -32602 | Correct mapping |
| 17.22 | `mcp_create_chain(title="x", prompt_ids=[<id>], separator="")` | JSON-RPC error -32602: separator must not be empty |

---

## 18. Versioning - Prompts & Scripts

**Sub-Agent:** SA-VERSION

Creates its own prompts and scripts for all tests (no dependency on bootstrap data).

| # | Case | Expected |
|---|------|----------|
| 18.1 | Create prompt -> edit title -> `GET /api/v1/versions/prompt/{id}` | 1 version with original title |
| 18.2 | Edit content -> list versions | 2 versions, ordered by version_number |
| 18.3 | `GET /api/v1/versions/{version_id}` | Returns specific version with title, content, description, notes, language snapshots |
| 18.4 | Create script -> edit content -> `GET /api/v1/script-versions/script/{id}` | 1 version with original content |
| 18.5 | Edit script title -> list script versions | 2 versions |
| 18.6 | `GET /api/v1/script-versions/{version_id}` | Returns specific script version |
| 18.7 | `POST /api/v1/versions/prompt/{id}/restore` with old version_id | Prompt restored; new version created |
| 18.8 | `POST /api/v1/script-versions/script/{id}/restore` with old version_id | Script restored; new version created |
| 18.9 | Update prompt with identical values (no change) | No new version created |
| 18.10 | Update script with identical values | No new version created |
| 18.11 | Edit prompt 5 times -> versions have numbers 1-5 | Monotonic version_number |
| 18.12 | `GET /api/v1/versions/prompt/99999` | 404 NOT_FOUND |
| 18.13 | `POST /api/v1/versions/prompt/{id}/restore` with non-existent version_id | 404 NOT_FOUND |
| 18.14 | Update only description (not title/content) -> check if version created | Version created (any field change triggers snapshot) |
| 18.15 | Update only notes -> version created? | Version created |
| 18.16 | Restore prompt to version 1, verify content matches original | Content matches version 1 snapshot |
| 18.17 | After restore: version list grows by 1 (restore creates a new snapshot) | version_number incremented |

---

## 19. Stress & Boundary Limits

**Sub-Agent:** SA-STRESS

Creates a dedicated `stress_user` for all tests.

| # | Case | Expected |
|---|------|----------|
| 19.1 | Title of exactly 200 characters (boundary) | 200, accepted |
| 19.2 | Title of 201 characters | 400 VALIDATION_ERROR |
| 19.3 | Content of exactly 1,048,576 bytes (1 MB) | 200, accepted |
| 19.4 | Content of 1,048,577 bytes (1 MB + 1 byte) | 400 VALIDATION_ERROR |
| 19.5 | Separator of exactly 100 characters | 200, accepted |
| 19.6 | Separator of 101 characters | 400 VALIDATION_ERROR |
| 19.7 | Taxonomy name of exactly 100 characters | 200, accepted |
| 19.8 | Taxonomy name of 101 characters | 400 VALIDATION_ERROR |
| 19.9 | Script language of exactly 30 characters | 200, accepted |
| 19.10 | Script language of 31 characters | 400 VALIDATION_ERROR |
| 19.11 | Username of only underscores `"___"` | 200, accepted |
| 19.12 | Language code `"en"` (valid) | 200, accepted |
| 19.13 | Language code `"e"` (1 char) | 400 VALIDATION_ERROR |
| 19.14 | Language code `"eng"` (3 chars) | 400 VALIDATION_ERROR |
| 19.15 | HTTP body > 2 MiB on normal endpoint (e.g., create prompt) | 413 Payload Too Large or body limit error |
| 19.16 | HTTP body > 10 MiB on import endpoint | Error: exceeds import limit |
| 19.17 | Pagination: `limit=1000` | 200, accepted (max) |
| 19.18 | Pagination: `limit=1001` | Capped to 1000 or error |
| 19.19 | Create 100 prompts rapidly (performance test) | All created successfully; list returns all 100 |
| 19.20 | Create chain with 50 steps | 200, accepted; content composition works |
| 19.21 | Create chain with 100 steps | Behavior documented (accepted or limit?) |
| 19.22 | Assign 50 tags to a single prompt | 200, accepted |
| 19.23 | **Rate limiting:** Send 120 requests within 60 seconds | All 120 succeed (limit is 120/60s) |
| 19.24 | **Rate limiting:** Send 121st request within same 60-second window | 429 Too Many Requests (or rate limit exceeded) |
| 19.25 | **Clipboard ring buffer:** Copy 51 entries via `POST /api/v1/clipboard/copy` -> GET history | History contains exactly 50 entries (oldest dropped) |
| 19.26 | `DELETE /api/v1/clipboard/history` | 200, history cleared |
| 19.27 | `GET /api/v1/clipboard/history` after clear | Empty array |
| 19.28 | Create prompt with content containing every Unicode plane (BMP, SMP, SIP) | 200, accepted; content retrieved correctly |
| 19.29 | Create prompt with content consisting of only zero-width characters | 200, accepted (not empty; has bytes) |
| 19.30 | Create prompt with content of repeating `"A" * 1_048_576` (exactly 1MB in ASCII) | 200, accepted |

---

## 20. Validation & Security Edge Cases

**Sub-Agent:** SA-STRESS

| # | Case | Expected |
|---|------|----------|
| 20.1 | SQL injection in prompt title: `"'; DROP TABLE prompts; --"` | 200, stored literally; `GET` returns exact string; tables intact |
| 20.2 | SQL injection in search query: `"' OR '1'='1"` | No crash; no extra results leaked |
| 20.3 | SQL injection in tag name: `"tag'; DELETE FROM tags; --"` | 200, stored literally |
| 20.4 | XSS in prompt content: `"<script>alert('xss')</script>"` | 200, stored and returned as-is (raw text, no sanitization needed) |
| 20.5 | XSS in prompt title: `"<img onerror=alert(1) src=x>"` | 200, stored literally |
| 20.6 | Path traversal in export: `"../../../etc/passwd"` | 403 PATH_TRAVERSAL |
| 20.7 | Path traversal in import: `"../../../../tmp/evil"` | 403 PATH_TRAVERSAL |
| 20.8 | Null byte in export path: `"export\x00.json"` | Error: invalid path |
| 20.9 | Path with `..` segments: `"/valid/path/../../../etc/shadow"` | 403 PATH_TRAVERSAL (canonicalized) |
| 20.10 | Symlink traversal (create symlink pointing outside sandbox) | 403 PATH_TRAVERSAL (symlinks resolved) |
| 20.11 | Negative ID: `GET /api/v1/prompts/-1` | 404 NOT_FOUND or type error |
| 20.12 | Float ID: `GET /api/v1/prompts/1.5` | 404 or type error (not a valid i64) |
| 20.13 | String ID: `GET /api/v1/prompts/abc` | 400 or 404 (path parameter parse failure) |
| 20.14 | Very large ID: `GET /api/v1/prompts/9999999999999999999` | 404 or overflow error |
| 20.15 | JSON with extra unknown fields: `{"title":"x","content":"y","evil":"payload"}` | Extra fields ignored; prompt created normally |
| 20.16 | Malformed JSON body: `{invalid json` | 400 Bad Request |
| 20.17 | Empty body on POST endpoint | 400 Bad Request |
| 20.18 | Wrong Content-Type header (text/plain instead of application/json) | 400 or 415 Unsupported Media Type |
| 20.19 | CRLF injection in header: `X-Custom: value\r\nEvil: header` | Header injection prevented by framework |
| 20.20 | Unicode normalization attack in username: `"u\u0308ser"` (u + combining diaeresis) vs `"\u00fc\x73er"` (precomposed) | Both treated as valid usernames; uniqueness handled correctly |
| 20.21 | Prompt title with only emoji: `"\ud83d\ude00\ud83d\ude01\ud83d\ude02"` | 200, accepted |
| 20.22 | Taxonomy name with newlines: `"tag\nwith\nnewlines"` | Accepted or rejected (document behavior) |
| 20.23 | Chain with circular reference via update (chain step pointing to itself) | Chains contain prompts/scripts, not other chains, so circular refs are architecturally impossible -- verify this |
| 20.24 | Concurrent updates to same prompt from two parallel requests | Both succeed; last-write-wins or conflict detection (document behavior) |
| 20.25 | `PUT /api/v1/prompts/{id}` with body containing prompt_id different from URL path | URL path ID takes precedence; body ID ignored or error |

---

## 21. End-to-End Workflows

**Sub-Agent:** SA-E2E (Phase 2, runs after all Phase 1 sub-agents complete)

Each case creates its own data. No dependency on bootstrap data beyond server availability.

| # | Case | Expected |
|---|------|----------|
| 21.1 | **Full prompt-to-chain-to-export flow:** Create user -> create tags -> create 3 prompts with tags -> create chain -> GET chain content -> export JSON -> verify export contains prompts, tags, chain | Export file is valid; contains all entities with correct associations |
| 21.2 | **Multi-user isolation workflow:** Create user_X and user_Y -> create prompt on user_X -> switch to user_Y -> search for user_X's prompt -> NOT found -> `copy-to-user` from user_X to user_Y -> switch to user_Y -> search -> found | Isolation holds; copy crosses boundary correctly |
| 21.3 | **Version lifecycle:** Create prompt -> edit title -> edit content -> edit description -> list versions (expect 3) -> restore to version 1 -> verify content matches original -> list versions (expect 4) | Full version chain intact; restore creates new snapshot |
| 21.4 | **Cascade delete verification:** Create user_D -> create 5 prompts, 3 scripts, 2 chains, 5 tags -> delete user_D -> verify ALL entities gone | Complete cascade; no orphaned data |
| 21.5 | **Chain protection flow:** Create prompt_P -> create chain_C using prompt_P -> attempt delete prompt_P -> 409 PROMPT_IN_USE -> delete chain_C -> attempt delete prompt_P again -> 200 success | Delete blocker works and is released correctly |
| 21.6 | **Script-in-chain protection:** Create script_S -> create mixed chain using script_S -> attempt delete script_S -> 409 SCRIPT_IN_USE -> delete chain -> delete script_S -> success | Same protection for scripts |
| 21.7 | **Template variable workflow:** Create prompt with `"Dear {{name}}, regarding {{topic}}: {{body}}"` -> add to chain -> GET raw content (variables intact) -> POST with variables `{name:"Alice",topic:"Rust",body:"..."}` -> verify substituted output | All variables substituted correctly |
| 21.8 | **Import/export round-trip:** Create user with 10 prompts (various tags, categories, favorites, archived) -> export JSON -> delete all prompts -> import JSON -> verify all 10 prompts restored with correct associations, favorites, archived state | Lossless round-trip |
| 21.9 | **MCP end-to-end:** Via MCP: create tag -> create category -> create prompt with associations -> create chain -> get chain content -> update prompt -> get chain content again (should reflect update) -> duplicate chain -> verify duplicate | Full MCP workflow without REST API |
| 21.10 | **Clipboard lifecycle:** Copy prompt A -> copy prompt B -> ... copy 51 prompts -> GET history -> verify 50 entries (ring buffer) -> verify oldest entry is prompt B (prompt A evicted) -> clear history -> GET history empty | Ring buffer semantics correct |
| 21.11 | **Ollama integration flow (skip if unavailable):** Check status -> set Ollama URL in settings -> improve a prompt -> translate the improved prompt -> autofill description -> verify all outputs are non-empty | Full LLM pipeline |
| 21.12 | **Stress E2E:** Create user -> create 50 prompts, 20 scripts, 10 chains (each with 5 steps) -> search for various terms -> export all -> delete user -> verify cascade cleanup | System stable under moderate load; cascade handles complex dependency graph |
| 21.13 | **Mixed chain composition with template variables:** Create prompt with `{{intro}}`, script with no variables, prompt with `{{conclusion}}` -> create mixed chain -> GET content (raw) -> POST with variables -> verify: prompt1 substituted, script literal, prompt2 substituted | Substitution only affects template-aware portions |

---

## 22. Web & GUI Endpoints

**Sub-Agent:** SA-WEB (Phase 2, runs after Phase 1)

| # | Case | Expected |
|---|------|----------|
| 22.1 | `GET /api/v1/web/setup/status` | 200, `{"is_first_run": <bool>, "data_dir": "<path>"}` |
| 22.2 | `POST /api/v1/web/setup/complete` | 200, `.setup_complete` marker file created |
| 22.3 | `GET /api/v1/web/setup/status` after marking complete | `is_first_run: false` |
| 22.4 | `GET /api/v1/web/doctor/probes` | 200, array of probes with `name`, `purpose`, `available`, `required`, `hint`, `link` fields |
| 22.5 | Verify doctor probes include Ollama dependency | At least one probe for Ollama with correct availability status |
| 22.6 | `GET /api/v1/web/mcp/status` | 200, returns registration status per target |
| 22.7 | `POST /api/v1/web/mcp/claude-code/install` | 200 or error (depending on Claude Code availability) |
| 22.8 | `POST /api/v1/web/mcp/claude-code/uninstall` | 200 or error |
| 22.9 | `POST /api/v1/web/mcp/invalid-target/install` | Error: invalid target |
| 22.10 | `GET /api/v1/web/ollama/status` | 200, `{"connected": <bool>}` |
| 22.11 | `GET /api/v1/web/ollama/models` | 200, `{"models": [...]}` (empty if Ollama unavailable) |
| 22.12 | `GET /api/v1/web/ollama/running` | 200, list of currently loaded models |
| 22.13 | `GET /api/v1/web/ollama/catalog` | 200, static curated model list with name, family, params, description |
| 22.14 | `POST /api/v1/web/ollama/pull` with `{"model": ""}` (empty) | Error: model name must not be empty |
| 22.15 | `POST /api/v1/web/ollama/pull` with valid model name | 202 ACCEPTED with `{"status": "pulling", "model": "<name>"}` (if Ollama available) |
| 22.16 | `POST /api/v1/web/ollama/delete` with `{"model": ""}` | Error: model name must not be empty |
| 22.17 | `POST /api/v1/web/ollama/show` with `{"model": ""}` | Error: model name must not be empty |
| 22.18 | `GET /api/v1/events/logs` (SSE endpoint) | Returns event stream with Content-Type: text/event-stream |
| 22.19 | `GET /api/v1/events/models` (SSE endpoint) | Returns event stream |
| 22.20 | Open 100 concurrent SSE connections to `/api/v1/events/logs` | All 100 connections accepted |
| 22.21 | Open 101st SSE connection | Connection rejected (max 100 SSE connections) |
| 22.22 | `POST /api/v1/web/dialog/save` with `{"title": "Save Test"}` | 200, `{"path": null}` in headless/browser mode (no native dialog) |
| 22.23 | `POST /api/v1/web/dialog/open-file` | 200, `{"path": null}` in headless mode |
| 22.24 | `POST /api/v1/web/dialog/open-dir` | 200, `{"path": null}` in headless mode |
| 22.25 | `POST /api/v1/shutdown` | Server initiates graceful shutdown (5s drain timeout) |
| 22.26 | Verify server is unreachable after shutdown | Connection refused on `GET /api/v1/health` |
| 22.27 | Verify log SSE event payload structure on `GET /api/v1/events/logs` | Event type is `"log"`, data is JSON with fields: `level` (string: trace/debug/info/warn/error), `target` (string: module path), `message` (string) |
| 22.28 | Verify model SSE event types during `POST /api/v1/web/ollama/pull` | Events include: `ollama_pull_progress` (fields: model, status, total, completed), `ollama_pull_complete` (fields: model), `ollama_pull_error` (fields: model, error) |
| 22.29 | SSE reconnection after client disconnect | New EventSource connection receives events; old connection slot is freed |
| 22.30 | SSE endpoints require valid session cookie | `GET /api/v1/events/logs` without session cookie returns 401 or empty stream (document behavior) |
| 22.31 | SSE keep-alive interval | Connection receives keep-alive comments (`: keep-alive`) at regular intervals (~15s) to prevent proxy timeouts |
| 22.32 | SSE connection count is decremented when client disconnects | After disconnect, the freed slot allows a new connection within the 100-connection limit |

---

## 23. Extended Feature Coverage

Sub-agent: **SA-FEATURES** -- Tests bulk operations, chain variables, template variable filters, version compare, settings PATCH, selective export, and search consistency.

### 23a: Bulk Operations

| Case | Action | Expected |
|------|--------|----------|
| 23.1 | `POST /api/v1/prompts/bulk-update` with `{"ids":[P1,P2,P3],"set_archived":true}` | 200, `{"updated":3}`, all three prompts are now archived |
| 23.2 | `POST /api/v1/prompts/bulk-update` with `{"ids":[P1,P2],"add_tag_ids":[T1]}` | 200, both prompts now have tag T1 |
| 23.3 | `POST /api/v1/prompts/bulk-update` with `{"ids":[P1],"remove_tag_ids":[T1]}` | 200, P1 no longer has tag T1 |
| 23.4 | `POST /api/v1/prompts/bulk-update` with `{"ids":[P1],"set_favorite":true,"add_category_ids":[C1],"add_collection_ids":[COL1]}` | 200, P1 is favorite + has category + has collection |
| 23.5 | `POST /api/v1/prompts/bulk-update` with `{"ids":[]}` | 400 VALIDATION_ERROR: ids must not be empty |
| 23.6 | `POST /api/v1/prompts/bulk-update` with `{"ids":[99999],"set_archived":true}` | 404 NOT_FOUND |
| 23.7 | `POST /api/v1/prompts/bulk-update` with `{"ids":[P1],"add_tag_ids":[user_B_tag]}` | 500 or error (cross-user tag blocked by DB trigger) |
| 23.8 | `POST /api/v1/scripts/bulk-update` with `{"ids":[S1,S2],"set_favorite":true}` | 200, both scripts favorited |
| 23.9 | `POST /api/v1/chains/bulk-update` with `{"ids":[CH1],"set_archived":true}` | 200, chain archived |

### 23b: Chain Variables Endpoint

| Case | Action | Expected |
|------|--------|----------|
| 23.10 | `GET /api/v1/chains/{chain_id}/variables` (chain with prompts containing `{{name}}` and `{{topic}}`) | 200, `{"variables":["name","topic"],"steps":[...]}` |
| 23.11 | `GET /api/v1/chains/{chain_id}/variables` (chain with no template variables) | 200, `{"variables":[],"steps":[...]}` |
| 23.12 | `GET /api/v1/chains/99999/variables` | 404 NOT_FOUND |
| 23.13 | Each step in the response includes `position`, `step_type`, `title`, `variables` | Verify structure matches spec |

### 23c: Template Variable Search Filter

| Case | Action | Expected |
|------|--------|----------|
| 23.14 | `POST /api/v1/prompts/search` with `{"user_id":N,"filter":{"has_variables":true}}` | Only prompts containing `{{...}}` returned |
| 23.15 | `POST /api/v1/prompts/search` with `{"user_id":N,"filter":{"has_variables":false}}` | Only prompts WITHOUT template variables |
| 23.16 | `POST /api/v1/prompts/search` with `{"user_id":N,"filter":{"variable_name":"topic"}}` | Only prompts containing `{{topic}}` |
| 23.17 | `POST /api/v1/prompts/search` with `{"user_id":N,"filter":{"variable_name":"nonexistent"}}` | Empty list |

### 23d: Version Compare

| Case | Action | Expected |
|------|--------|----------|
| 23.18 | `GET /api/v1/versions/compare?version_a=V1&version_b=V2` (same prompt) | 200, `{"version_a":{...},"version_b":{...}}` |
| 23.19 | `GET /api/v1/versions/compare?version_a=V1&version_b=V_OTHER_PROMPT` (different prompts) | 400 VALIDATION_ERROR |
| 23.20 | `GET /api/v1/versions/compare?version_a=99999&version_b=1` | 404 NOT_FOUND |
| 23.21 | `GET /api/v1/script-versions/compare?version_a=SV1&version_b=SV2` | 200, both script versions returned |

### 23e: Restore with version_id

| Case | Action | Expected |
|------|--------|----------|
| 23.22 | `POST .../restore` with `{"version_number":1}` | 200, prompt restored to version 1 |
| 23.23 | `POST .../restore` with `{"version_id":V_ID}` | 200, prompt restored to that version |
| 23.24 | `POST .../restore` with `{"version_number":1,"version_id":42}` | 400 VALIDATION_ERROR: exactly one must be set |
| 23.25 | `POST .../restore` with `{}` | 400 VALIDATION_ERROR: exactly one must be set |

### 23f: Settings Partial Update (PATCH)

| Case | Action | Expected |
|------|--------|----------|
| 23.26 | `PATCH /api/v1/settings/user` with `{"user_id":N,"theme":"dark"}` | 200, only theme changed, all other settings preserved |
| 23.27 | `PATCH /api/v1/settings/user` with `{"user_id":N,"ollama_base_url":"http://evil.com"}` | 400 VALIDATION_ERROR (SSRF protection) |
| 23.28 | `PATCH /api/v1/settings/user` with `{"user_id":N,"ollama_model":null}` | 200, model cleared to null |
| 23.29 | `PUT /api/v1/settings/user` with full body | Still works (backward compatibility) |

### 23g: Export All

| Case | Action | Expected |
|------|--------|----------|
| 23.30 | `POST /api/v1/io/export/json` with `{"user_id":N,"path":"/tmp/test.json","prompt_ids":[]}` | 200, exports ALL user prompts |
| 23.31 | `POST /api/v1/io/export/json` with `{"user_id":N,"path":"/tmp/test.json"}` (prompt_ids absent) | 200, exports ALL user prompts |
| 23.32 | `POST /api/v1/io/export/json` with `{"user_id":N,"path":"/tmp/test.json","prompt_ids":[P1]}` | 200, exports only P1 |

### 23h: Search API Consistency

| Case | Action | Expected |
|------|--------|----------|
| 23.33 | `POST /api/v1/search/scripts` with `{"user_id":N,"query":"test","limit":2}` | Max 2 results (top-level limit honored) |
| 23.34 | `POST /api/v1/search/scripts` with `{"user_id":N,"query":"test","offset":5}` | Results offset by 5 |
| 23.35 | `POST /api/v1/search/chains` with `{"user_id":N,"query":"test","filter":{"is_favorite":true}}` | Only favorite chains (nested filter, not flattened) |
| 23.36 | `POST /api/v1/search/chains` with `{"user_id":N,"query":"test","limit":1}` | Max 1 result |

---

## 24. Session Management

**Sub-Agent:** SA-SESSION

All session tests must validate the HTTP cookie-based authentication model. The `np_session` cookie is HttpOnly, SameSite=Strict, Path=/. Sessions are per-client, not global.

| # | Case | Expected |
|---|------|----------|
| 24.1 | `POST /api/v1/sessions` with `{"user_id": <user_A_id>}` | 201 CREATED, response includes `Set-Cookie: np_session=<token>; HttpOnly; SameSite=Strict; Path=/` header |
| 24.2 | `GET /api/v1/sessions/me` with valid session cookie | 200, returns `{"user": {"id": N, "username": "...", "display_name": "..."}, "remaining_ttl_secs": N}` |
| 24.3 | `GET /api/v1/sessions/me` without any session cookie | 200, returns `{"user": null, "remaining_ttl_secs": null}` (endpoint does NOT require auth) |
| 24.4 | `GET /api/v1/sessions/me` with expired or invalid session cookie | 200, returns `{"user": null, "remaining_ttl_secs": null}` |
| 24.5 | `PUT /api/v1/sessions/switch` with `{"user_id": <user_B_id>}` and valid session | 200, `{"ok": true}`, session now references user_B |
| 24.6 | `GET /api/v1/sessions/me` after switch | Returns user_B details (id, username, display_name) |
| 24.7 | `PUT /api/v1/sessions/switch` with non-existent user_id | 404 NOT_FOUND |
| 24.8 | `PUT /api/v1/sessions/switch` without valid session cookie | 401 AUTH_REQUIRED: "session not found" |
| 24.9 | `DELETE /api/v1/sessions` with valid session | 200, `{"ok": true}`, response includes `Set-Cookie: np_session=; ... Max-Age=0` (cookie cleared) |
| 24.10 | `GET /api/v1/sessions/me` after logout | Returns `{"user": null, "remaining_ttl_secs": null}` |
| 24.11 | `POST /api/v1/sessions` with non-existent user_id (99999) | 404 NOT_FOUND |
| 24.12 | After `POST /api/v1/sessions`, verify `GET /api/v1/settings/app/last_user_id` returns the same user_id | `last_user_id` is persisted on session creation |
| 24.13 | `DELETE /api/v1/sessions` without valid session | 401 AUTH_REQUIRED (AuthSession extractor rejects) |
| 24.14 | Create two sessions for different users concurrently (separate cookie jars) | Both sessions active and isolated; `/sessions/me` on each returns the correct user |
| 24.15 | Verify session cookie is HttpOnly and SameSite=Strict | Inspect `Set-Cookie` header on `POST /api/v1/sessions` response |

---

## 25. Count & Languages Endpoints

**Sub-Agent:** SA-COUNTS

These endpoints provide aggregate statistics per user. They are used by the frontend status bar and sidebar.

| # | Case | Expected |
|---|------|----------|
| 25.1 | `GET /api/v1/prompts/count` (authenticated as user_A) | 200, `{"count": N}` where N matches the number of user_A's prompts |
| 25.2 | `GET /api/v1/prompts/count` without session cookie | 401 AUTH_REQUIRED |
| 25.3 | `GET /api/v1/prompts/languages` (authenticated as user_A) | 200, list of distinct ISO 639-1 language codes used in user_A's prompts |
| 25.4 | `GET /api/v1/prompts/languages` when no prompts have a language set | 200, empty list `[]` |
| 25.5 | `GET /api/v1/scripts/count` (authenticated as user_A) | 200, `{"count": N}` matching user_A's script count |
| 25.6 | `GET /api/v1/scripts/languages` (authenticated as user_A) | 200, list of distinct `script_language` values used across user_A's scripts |
| 25.7 | `GET /api/v1/chains/count` (authenticated as user_A) | 200, `{"count": N}` matching user_A's chain count |
| 25.8 | Create 3 new prompts, then `GET /api/v1/prompts/count` | Count incremented by 3 compared to baseline |
| 25.9 | Delete a prompt, then `GET /api/v1/prompts/count` | Count decremented by 1 |
| 25.10 | `GET /api/v1/prompts/count` without session | 401 AUTH_REQUIRED |
| 25.11 | `GET /api/v1/scripts/languages` with scripts in python, bash, javascript | Returns `["bash", "javascript", "python"]` (or unordered equivalent) |
| 25.12 | `GET /api/v1/chains/count` after creating and deleting a chain | Count is accurate (reflects net change) |

---

## Regression References

The following reference IDs track known issues that MUST be verified in every test run:

| REF | Section | Description |
|-----|---------|-------------|
| REF-001 | 3.14 | Prompt in chain: delete must return 409 PROMPT_IN_USE with chain_titles |
| REF-002 | 5.15 | Script in chain: delete must return 409 SCRIPT_IN_USE with chain_titles |
| REF-003 | 4.4 / 18.9 | Update with identical values must NOT create a new version |
| REF-004 | 12.11 | User delete must CASCADE to all dependent entities |
| REF-005 | 13.7-13.13 | Ollama URL SSRF protection: only localhost/127.0.0.1/::1 allowed |
| REF-006 | 14.13-14.14 | Path traversal protection on import/export |
| REF-007 | 19.23-19.24 | Rate limiting: 120 req/60s per IP |
| REF-008 | 16.19-16.20 | MCP access boundary: no delete, no Ollama |
| REF-009 | 16.3 / 17.17-17.18 | MCP user isolation: mcp_agent sees only its own data |
| REF-010 | 19.25 | Clipboard ring buffer: max 50 entries, FIFO eviction |
| REF-011 | 24.1-24.9 | Session lifecycle: create, switch, inspect, logout with proper cookie handling |
| REF-012 | 13.16 | App settings write restriction: only `last_user_id` is in the writable allowlist; other keys return 403 |
| REF-013 | 12.24-12.25 | Chain copy-to-user: deep copy with step remapping to new entity IDs |
| REF-014 | 25.1, 25.5, 25.7 | Count endpoints return accurate entity counts per user |
