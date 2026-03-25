# REST API Reference

Base URL: `http://localhost:3030`

All endpoints under `/api/v1/` require session authentication via the
`neuronprompter_session` cookie, except where noted. The session cookie is
obtained by calling the session creation endpoint.

---

## System

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/health` | Health check (no authentication required) |
| POST | `/api/v1/shutdown` | Shut down the server (not available in headless/serve mode) |

## Sessions

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/sessions` | Create a session (login; no authentication required) |
| PUT | `/api/v1/sessions/switch` | Switch the active session to a different user |
| DELETE | `/api/v1/sessions` | Destroy the current session (logout) |
| GET | `/api/v1/sessions/me` | Return the current session details |

## Users

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/users` | List all users |
| POST | `/api/v1/users` | Create a user |
| PUT | `/api/v1/users/{user_id}` | Update a user |
| PUT | `/api/v1/users/{user_id}/switch` | Switch the active user |
| DELETE | `/api/v1/users/{user_id}` | Delete a user |

## Prompts

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/prompts/search` | List prompts with search/filter criteria |
| POST | `/api/v1/prompts` | Create a prompt |
| GET | `/api/v1/prompts/{prompt_id}` | Get a prompt by ID |
| PUT | `/api/v1/prompts/{prompt_id}` | Update a prompt |
| DELETE | `/api/v1/prompts/{prompt_id}` | Delete a prompt |
| POST | `/api/v1/prompts/{prompt_id}/duplicate` | Duplicate a prompt |
| PATCH | `/api/v1/prompts/{prompt_id}/favorite` | Toggle the favorite flag on a prompt |
| PATCH | `/api/v1/prompts/{prompt_id}/archive` | Toggle the archive flag on a prompt |
| POST | `/api/v1/prompts/bulk-update` | Bulk-update multiple prompts |
| GET | `/api/v1/prompts/count` | Return the total prompt count |
| GET | `/api/v1/prompts/languages` | List distinct languages used in prompts |
| POST | `/api/v1/search/prompts` | Full-text search across prompts |

## Scripts

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/scripts/search` | List scripts with search/filter criteria |
| POST | `/api/v1/scripts` | Create a script |
| POST | `/api/v1/scripts/sync` | Synchronize scripts from the filesystem |
| POST | `/api/v1/scripts/import-file` | Import a script from a file |
| GET | `/api/v1/scripts/{script_id}` | Get a script by ID |
| PUT | `/api/v1/scripts/{script_id}` | Update a script |
| DELETE | `/api/v1/scripts/{script_id}` | Delete a script |
| POST | `/api/v1/scripts/{script_id}/duplicate` | Duplicate a script |
| PATCH | `/api/v1/scripts/{script_id}/favorite` | Toggle the favorite flag on a script |
| PATCH | `/api/v1/scripts/{script_id}/archive` | Toggle the archive flag on a script |
| POST | `/api/v1/scripts/bulk-update` | Bulk-update multiple scripts |
| GET | `/api/v1/scripts/count` | Return the total script count |
| GET | `/api/v1/scripts/languages` | List distinct languages used in scripts |
| POST | `/api/v1/search/scripts` | Full-text search across scripts |

## Chains

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/chains/search` | List chains with search/filter criteria |
| POST | `/api/v1/chains` | Create a chain |
| GET | `/api/v1/chains/{chain_id}` | Get a chain by ID |
| PUT | `/api/v1/chains/{chain_id}` | Update a chain |
| DELETE | `/api/v1/chains/{chain_id}` | Delete a chain |
| POST | `/api/v1/chains/{chain_id}/duplicate` | Duplicate a chain |
| PATCH | `/api/v1/chains/{chain_id}/favorite` | Toggle the favorite flag on a chain |
| PATCH | `/api/v1/chains/{chain_id}/archive` | Toggle the archive flag on a chain |
| GET | `/api/v1/chains/{chain_id}/content` | Get the composed content of a chain |
| GET | `/api/v1/chains/{chain_id}/variables` | Get the template variables defined in a chain |
| GET | `/api/v1/chains/by-prompt/{prompt_id}` | List chains that reference a given prompt |
| POST | `/api/v1/chains/bulk-update` | Bulk-update multiple chains |
| GET | `/api/v1/chains/count` | Return the total chain count |
| POST | `/api/v1/search/chains` | Full-text search across chains |

## Copy (Cross-User)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/prompts/{prompt_id}/copy-to-user` | Copy a prompt to another user |
| POST | `/api/v1/scripts/{script_id}/copy-to-user` | Copy a script to another user |
| POST | `/api/v1/chains/{chain_id}/copy-to-user` | Copy a chain to another user |
| POST | `/api/v1/users/bulk-copy` | Bulk-copy all items to another user |

## Tags

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/tags/user/{user_id}` | List all tags for a user |
| POST | `/api/v1/tags` | Create a tag |
| PUT | `/api/v1/tags/{tag_id}` | Rename a tag |
| DELETE | `/api/v1/tags/{tag_id}` | Delete a tag |

## Collections

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/collections/user/{user_id}` | List all collections for a user |
| POST | `/api/v1/collections` | Create a collection |
| PUT | `/api/v1/collections/{collection_id}` | Rename a collection |
| DELETE | `/api/v1/collections/{collection_id}` | Delete a collection |

## Categories

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/categories/user/{user_id}` | List all categories for a user |
| POST | `/api/v1/categories` | Create a category |
| PUT | `/api/v1/categories/{category_id}` | Rename a category |
| DELETE | `/api/v1/categories/{category_id}` | Delete a category |

## Prompt Versions

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/versions/prompt/{prompt_id}` | List all versions of a prompt |
| GET | `/api/v1/versions/{version_id}` | Get a specific prompt version |
| POST | `/api/v1/versions/prompt/{prompt_id}/restore` | Restore a prompt to a previous version |
| GET | `/api/v1/versions/compare` | Compare two prompt versions |

## Script Versions

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/script-versions/script/{script_id}` | List all versions of a script |
| GET | `/api/v1/script-versions/{version_id}` | Get a specific script version |
| POST | `/api/v1/script-versions/script/{script_id}/restore` | Restore a script to a previous version |
| GET | `/api/v1/script-versions/compare` | Compare two script versions |

## Import / Export

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/io/export/json` | Export data as JSON |
| POST | `/api/v1/io/import/json` | Import data from JSON (body limit: 10 MiB) |
| POST | `/api/v1/io/export/markdown` | Export data as Markdown |
| POST | `/api/v1/io/import/markdown` | Import data from Markdown (body limit: 10 MiB) |
| POST | `/api/v1/io/backup` | Create a database backup |

## Clipboard

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/clipboard/copy` | Copy content to the system clipboard |
| POST | `/api/v1/clipboard/copy-substituted` | Copy content to the clipboard with variable substitution |
| GET | `/api/v1/clipboard/history` | Get the clipboard copy history |
| DELETE | `/api/v1/clipboard/history` | Clear the clipboard copy history |

## Settings

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/settings/db-path` | Get the database file path |
| GET | `/api/v1/settings/app/{key}` | Get an application-level setting by key |
| PUT | `/api/v1/settings/app/{key}` | Set an application-level setting by key |
| GET | `/api/v1/settings/user/{user_id}` | Get all settings for a user |
| PUT | `/api/v1/settings/user` | Replace the current user's settings |
| PATCH | `/api/v1/settings/user` | Partially update the current user's settings |

## Ollama (API-Level)

These endpoints are registered in the core API router and are available in
both GUI and headless modes.

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/ollama/status` | Check Ollama connectivity status |
| POST | `/api/v1/ollama/improve` | Request an Ollama-powered prompt improvement |
| POST | `/api/v1/ollama/translate` | Request an Ollama-powered prompt translation |
| POST | `/api/v1/ollama/autofill` | Request Ollama-powered metadata autofill |

## Server-Sent Events (Web Only)

These endpoints are registered by the web crate and are only available when
the frontend is served (not in headless API-only mode).

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/events/logs` | SSE stream of server log events |
| GET | `/api/v1/events/models` | SSE stream of model operation events |

## Setup (Web Only)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/web/setup/status` | Get the first-run setup status |
| POST | `/api/v1/web/setup/complete` | Mark the first-run setup as complete |

## Doctor (Web Only)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/web/doctor/probes` | Run dependency and health probes |

## MCP Registration (Web Only)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/web/mcp/status` | Get MCP registration status for all targets |
| POST | `/api/v1/web/mcp/{target}/install` | Install the MCP server configuration for a target |
| POST | `/api/v1/web/mcp/{target}/uninstall` | Uninstall the MCP server configuration for a target |

## Ollama Model Management (Web Only)

These endpoints are registered by the web crate and provide model lifecycle
management through the frontend.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/web/ollama/status` | Check Ollama daemon availability |
| GET | `/api/v1/web/ollama/models` | List locally available Ollama models |
| GET | `/api/v1/web/ollama/running` | List currently running Ollama models |
| GET | `/api/v1/web/ollama/catalog` | List models available in the Ollama catalog |
| POST | `/api/v1/web/ollama/pull` | Pull (download) an Ollama model |
| POST | `/api/v1/web/ollama/delete` | Delete a local Ollama model |
| POST | `/api/v1/web/ollama/show` | Show details of an Ollama model |

## Native Dialogs (Web Only)

These endpoints proxy native OS file dialogs and are only functional when
running in GUI mode (`native_dialogs: true`).

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/web/dialog/save` | Open a native save-file dialog |
| POST | `/api/v1/web/dialog/open-file` | Open a native open-file dialog |
| POST | `/api/v1/web/dialog/open-dir` | Open a native open-directory dialog |

---

## Notes

- The default request body limit is 2 MiB. Import endpoints under `/api/v1/io/`
  allow up to 10 MiB.
- The `/api/v1/shutdown` endpoint is omitted in headless (serve) mode. Use
  `Ctrl+C` or `SIGTERM` to stop the server instead.
- Web-only endpoints (prefixed with `/api/v1/web/` or `/api/v1/events/`) are
  registered by the `neuronprompter-web` crate and are not available when
  running the API server standalone.
- Path parameters use the `{param}` syntax (e.g., `{prompt_id}`, `{user_id}`).
- CORS is restricted to the configured origin. In development mode (`--dev`),
  common localhost origins on ports 3000, 3030, and 5173 are permitted.
