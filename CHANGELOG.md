# Changelog

All notable changes to NeuronPrompter are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- Close button on login page for native frameless window
- Example content seeding in first-run welcome flow
- Unsaved-changes guard for tab and item navigation
- Chain filtering by taxonomy (tags, categories, collections)

### Fixed

- 401 AUTH_REQUIRED during welcome flow when marker file missing
- Login page scrollable when many users exist
- 14 defects from parallel verification audit
- Audit findings across docs, workflows, and metadata
- LaTeX overfull boxes in architecture document

### Security

- Remove database path from unauthenticated health endpoint
- Restrict setup status data directory to first-run responses
- Add authentication to MCP status endpoint
- Add user-existence guard to setup completion endpoint
- Strip timestamps from public user list endpoint
- Validate import/export paths against user home directory
- Sanitize internal error details in API responses

## [0.1.0] - 2026-03-24

Initial release.

### Added

- REST API server (Axum) with 94 endpoints, session-based authentication,
  per-IP rate limiting, CORS, and Server-Sent Events for real-time log streaming
- SolidJS web frontend with 8 tabs (Prompts, Scripts, Chains, Organize, Models,
  Settings, Users, Clipboard) embedded in the binary via rust-embed
- Native GUI window via tao/wry (WebView2 on Windows, WebKit on macOS) with
  browser fallback on Linux and when the gui feature is disabled
- MCP server with 23 tools for Claude Code and Claude Desktop App integration
  (JSON-RPC 2.0 over stdio)
- Prompt management with CRUD operations, favorites, archiving, duplication,
  full-text search (FTS5), template variables (`{{var}}`), and version history
- Script management with language-tagged code storage, file system sync from
  local Markdown files with YAML frontmatter, and version history
- Chain system for composing ordered sequences of prompts and scripts with
  custom separators and variable resolution
- Taxonomy system with tags, categories, and collections per user
- Multi-user support with session management, user switching, and cross-user
  content copying
- Ollama LLM integration for prompt improvement, translation, and metadata
  derivation
- Import/Export in JSON and Markdown (with YAML frontmatter) formats
- Database backup to timestamped SQLite copies
- Clipboard history with per-user copy tracking (50-entry ring buffer)
- CI pipeline with rustfmt, clippy, cargo test, frontend lint/test/build, and
  cargo-deny license auditing
- Pre-commit hooks for formatting, linting, testing, and architecture validation

[Unreleased]: https://github.com/FF-TEC/NeuronPrompter/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/FF-TEC/NeuronPrompter/releases/tag/v0.1.0
