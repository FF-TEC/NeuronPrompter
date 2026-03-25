-- =============================================================================
-- NeuronPrompter -- Complete Schema (Consolidated + Hardened)
--
-- Single migration containing the complete data model with full integrity
-- hardening: CHECK constraints on all value-domain fields, COLLATE NOCASE
-- on taxonomy names, cross-user ownership triggers on all junction tables
-- and chain_steps, json_valid on the extra field, and FTS5 with optimized
-- synchronization triggers.
--
-- All CREATE TABLE and CREATE INDEX statements use IF NOT EXISTS as
-- defense-in-depth for crash recovery scenarios where the migration
-- tracking table was not updated but some DDL was already applied.
--
-- Entity groups:
--   1. Users and settings
--   2. Prompts with versions, taxonomy, and FTS5
--   3. Scripts with versions, taxonomy, filesystem sync, and FTS5
--   4. Chains with polymorphic steps, taxonomy, and FTS5
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Users: local profiles owning prompts, tags, categories, and collections.
-- Profiles are convenience separations, not security boundaries.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    username     TEXT    NOT NULL UNIQUE CHECK(length(trim(username)) > 0),
    display_name TEXT    NOT NULL CHECK(length(trim(display_name)) > 0),
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- ---------------------------------------------------------------------------
-- Tags: user-scoped labels for cross-cutting classification.
-- COLLATE NOCASE ensures case-insensitive uniqueness per user.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tags (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL COLLATE NOCASE CHECK(length(trim(name)) > 0),
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(user_id, name)
);

-- ---------------------------------------------------------------------------
-- Collections: user-scoped named groups for organizing entities by context.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS collections (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL COLLATE NOCASE CHECK(length(trim(name)) > 0),
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(user_id, name)
);

-- ---------------------------------------------------------------------------
-- Categories: user-scoped thematic classification.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS categories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL COLLATE NOCASE CHECK(length(trim(name)) > 0),
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(user_id, name)
);

-- ---------------------------------------------------------------------------
-- Application-wide settings: key-value store independent of user profiles.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ---------------------------------------------------------------------------
-- Per-user preferences: structured table for typed, validatable settings.
-- last_collection_id references collections with ON DELETE SET NULL.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS user_settings (
    user_id             INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    theme               TEXT    NOT NULL DEFAULT 'system' CHECK(theme IN ('light', 'dark', 'system')),
    last_collection_id  INTEGER REFERENCES collections(id) ON DELETE SET NULL,
    sidebar_collapsed   INTEGER NOT NULL DEFAULT 0 CHECK(sidebar_collapsed IN (0, 1)),
    sort_field          TEXT    NOT NULL DEFAULT 'updated_at' CHECK(sort_field IN ('updated_at', 'created_at', 'title')),
    sort_direction      TEXT    NOT NULL DEFAULT 'desc' CHECK(sort_direction IN ('asc', 'desc')),
    ollama_base_url     TEXT    NOT NULL DEFAULT 'http://localhost:11434',
    ollama_model        TEXT,
    extra               TEXT    NOT NULL DEFAULT '{}' CHECK(json_valid(extra) AND json_type(extra) = 'object')
);

-- ===========================================================================
-- Prompts
-- ===========================================================================

CREATE TABLE IF NOT EXISTS prompts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title           TEXT    NOT NULL CHECK(length(trim(title)) > 0),
    content         TEXT    NOT NULL,
    description     TEXT,
    notes           TEXT,
    language        TEXT    CHECK(language IS NULL OR length(language) <= 2),
    is_favorite     INTEGER NOT NULL DEFAULT 0 CHECK(is_favorite IN (0, 1)),
    is_archived     INTEGER NOT NULL DEFAULT 0 CHECK(is_archived IN (0, 1)),
    current_version INTEGER NOT NULL DEFAULT 1 CHECK(current_version >= 1),
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS prompt_versions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    prompt_id      INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL CHECK(version_number >= 1),
    title          TEXT    NOT NULL,
    content        TEXT    NOT NULL,
    description    TEXT,
    notes          TEXT,
    language       TEXT    CHECK(language IS NULL OR length(language) <= 2),
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(prompt_id, version_number)
);

-- Junction tables: prompts <-> taxonomy
CREATE TABLE IF NOT EXISTS prompt_tags (
    prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    tag_id    INTEGER NOT NULL REFERENCES tags(id)    ON DELETE CASCADE,
    PRIMARY KEY (prompt_id, tag_id)
);

CREATE TABLE IF NOT EXISTS prompt_collections (
    prompt_id     INTEGER NOT NULL REFERENCES prompts(id)      ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id)  ON DELETE CASCADE,
    PRIMARY KEY (prompt_id, collection_id)
);

CREATE TABLE IF NOT EXISTS prompt_categories (
    prompt_id   INTEGER NOT NULL REFERENCES prompts(id)     ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id)  ON DELETE CASCADE,
    PRIMARY KEY (prompt_id, category_id)
);

-- ===========================================================================
-- Scripts
-- ===========================================================================

CREATE TABLE IF NOT EXISTS scripts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title           TEXT    NOT NULL CHECK(length(trim(title)) > 0),
    content         TEXT    NOT NULL,
    description     TEXT,
    notes           TEXT,
    script_language TEXT    NOT NULL CHECK(length(script_language) BETWEEN 1 AND 30),
    language        TEXT    CHECK(language IS NULL OR length(language) <= 2),
    is_favorite     INTEGER NOT NULL DEFAULT 0 CHECK(is_favorite IN (0, 1)),
    is_archived     INTEGER NOT NULL DEFAULT 0 CHECK(is_archived IN (0, 1)),
    current_version INTEGER NOT NULL DEFAULT 1 CHECK(current_version >= 1),
    source_path     TEXT    CHECK(source_path IS NULL OR length(trim(source_path)) > 0),
    is_synced       INTEGER NOT NULL DEFAULT 0 CHECK(is_synced IN (0, 1)),
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS script_versions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id       INTEGER NOT NULL REFERENCES scripts(id) ON DELETE CASCADE,
    version_number  INTEGER NOT NULL CHECK(version_number >= 1),
    title           TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    description     TEXT,
    notes           TEXT,
    script_language TEXT    NOT NULL CHECK(length(script_language) BETWEEN 1 AND 30),
    language        TEXT    CHECK(language IS NULL OR length(language) <= 2),
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(script_id, version_number)
);

-- Junction tables: scripts <-> taxonomy
CREATE TABLE IF NOT EXISTS script_tags (
    script_id INTEGER NOT NULL REFERENCES scripts(id) ON DELETE CASCADE,
    tag_id    INTEGER NOT NULL REFERENCES tags(id)     ON DELETE CASCADE,
    PRIMARY KEY (script_id, tag_id)
);

CREATE TABLE IF NOT EXISTS script_categories (
    script_id   INTEGER NOT NULL REFERENCES scripts(id)    ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    PRIMARY KEY (script_id, category_id)
);

CREATE TABLE IF NOT EXISTS script_collections (
    script_id     INTEGER NOT NULL REFERENCES scripts(id)     ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (script_id, collection_id)
);

-- ===========================================================================
-- Chains
-- ===========================================================================

CREATE TABLE IF NOT EXISTS chains (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT    NOT NULL CHECK(length(trim(title)) > 0),
    description TEXT,
    notes       TEXT,
    language    TEXT    CHECK(language IS NULL OR length(language) <= 2),
    separator   TEXT    NOT NULL DEFAULT '\n\n',
    is_favorite INTEGER NOT NULL DEFAULT 0 CHECK(is_favorite IN (0, 1)),
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK(is_archived IN (0, 1)),
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS chain_steps (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    chain_id  INTEGER NOT NULL REFERENCES chains(id)  ON DELETE CASCADE,
    step_type TEXT    NOT NULL DEFAULT 'prompt' CHECK(step_type IN ('prompt', 'script')),
    prompt_id INTEGER REFERENCES prompts(id) ON DELETE RESTRICT,
    script_id INTEGER REFERENCES scripts(id) ON DELETE RESTRICT,
    position  INTEGER NOT NULL CHECK(position >= 0),
    UNIQUE(chain_id, position),
    CHECK(
        (step_type = 'prompt' AND prompt_id IS NOT NULL AND script_id IS NULL) OR
        (step_type = 'script' AND script_id IS NOT NULL AND prompt_id IS NULL)
    )
);

-- Junction tables: chains <-> taxonomy
CREATE TABLE IF NOT EXISTS chain_tags (
    chain_id INTEGER NOT NULL REFERENCES chains(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
    PRIMARY KEY (chain_id, tag_id)
);

CREATE TABLE IF NOT EXISTS chain_categories (
    chain_id    INTEGER NOT NULL REFERENCES chains(id)     ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    PRIMARY KEY (chain_id, category_id)
);

CREATE TABLE IF NOT EXISTS chain_collections (
    chain_id      INTEGER NOT NULL REFERENCES chains(id)      ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (chain_id, collection_id)
);

-- ===========================================================================
-- Secondary Indexes
--
-- All indexes use IF NOT EXISTS for idempotent application during crash
-- recovery scenarios.
-- ===========================================================================

-- Prompts
CREATE INDEX IF NOT EXISTS idx_prompts_user     ON prompts(user_id);
CREATE INDEX IF NOT EXISTS idx_prompts_favorite ON prompts(user_id, is_favorite);
CREATE INDEX IF NOT EXISTS idx_prompts_archived ON prompts(user_id, is_archived);
CREATE INDEX IF NOT EXISTS idx_prompts_updated  ON prompts(user_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_versions_prompt  ON prompt_versions(prompt_id, version_number);
CREATE INDEX IF NOT EXISTS idx_pt_tag           ON prompt_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_pc_collection    ON prompt_collections(collection_id);
CREATE INDEX IF NOT EXISTS idx_pcat_category    ON prompt_categories(category_id);

-- Tags, collections, categories
CREATE INDEX IF NOT EXISTS idx_tags_user        ON tags(user_id);
CREATE INDEX IF NOT EXISTS idx_collections_user ON collections(user_id);
CREATE INDEX IF NOT EXISTS idx_categories_user  ON categories(user_id);

-- Scripts
CREATE INDEX IF NOT EXISTS idx_scripts_user     ON scripts(user_id);
CREATE INDEX IF NOT EXISTS idx_scripts_favorite ON scripts(user_id, is_favorite);
CREATE INDEX IF NOT EXISTS idx_scripts_archived ON scripts(user_id, is_archived);
CREATE INDEX IF NOT EXISTS idx_scripts_updated  ON scripts(user_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_scripts_synced   ON scripts(user_id, is_synced);
CREATE INDEX IF NOT EXISTS idx_script_versions_script ON script_versions(script_id, version_number);
CREATE INDEX IF NOT EXISTS idx_st_tag          ON script_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_sc_collection   ON script_collections(collection_id);
CREATE INDEX IF NOT EXISTS idx_scat_category   ON script_categories(category_id);

-- Chains
CREATE INDEX IF NOT EXISTS idx_chains_user     ON chains(user_id);
CREATE INDEX IF NOT EXISTS idx_chains_updated  ON chains(user_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_chains_favorite ON chains(user_id, is_favorite);
CREATE INDEX IF NOT EXISTS idx_chains_archived ON chains(user_id, is_archived);
CREATE INDEX IF NOT EXISTS idx_chain_steps_chain  ON chain_steps(chain_id, position);
CREATE INDEX IF NOT EXISTS idx_chain_steps_prompt ON chain_steps(prompt_id);
CREATE INDEX IF NOT EXISTS idx_chain_steps_script ON chain_steps(script_id);
CREATE INDEX IF NOT EXISTS idx_ct_tag        ON chain_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_cc_collection ON chain_collections(collection_id);
CREATE INDEX IF NOT EXISTS idx_ccat_category ON chain_categories(category_id);

-- ===========================================================================
-- Ownership Validation Triggers
--
-- Ensure all linked entities belong to the same user. INSERT and UPDATE
-- triggers on every junction table, chain_steps, and user_settings prevent
-- cross-user data associations at the database level.
-- ===========================================================================

-- Prompt junction tables
CREATE TRIGGER validate_prompt_tag_ownership BEFORE INSERT ON prompt_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and tag belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_prompt_category_ownership BEFORE INSERT ON prompt_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and category belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_prompt_collection_ownership BEFORE INSERT ON prompt_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and collection belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

-- Script junction tables
CREATE TRIGGER validate_script_tag_ownership BEFORE INSERT ON script_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and tag belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_script_category_ownership BEFORE INSERT ON script_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and category belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_script_collection_ownership BEFORE INSERT ON script_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and collection belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

-- Chain junction tables
CREATE TRIGGER validate_chain_tag_ownership BEFORE INSERT ON chain_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and tag belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_chain_category_ownership BEFORE INSERT ON chain_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and category belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_chain_collection_ownership BEFORE INSERT ON chain_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and collection belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

-- Chain steps: polymorphic ownership check
CREATE TRIGGER validate_chain_step_prompt_ownership BEFORE INSERT ON chain_steps
WHEN NEW.prompt_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and prompt belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM prompts WHERE id = NEW.prompt_id);
END;

CREATE TRIGGER validate_chain_step_script_ownership BEFORE INSERT ON chain_steps
WHEN NEW.script_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and script belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM scripts WHERE id = NEW.script_id);
END;

-- User settings: last_collection_id ownership (INSERT + UPDATE)
CREATE TRIGGER validate_user_settings_collection_ownership_insert
BEFORE INSERT ON user_settings
WHEN NEW.last_collection_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: collection belongs to different user')
    WHERE (SELECT user_id FROM collections WHERE id = NEW.last_collection_id)
       != NEW.user_id;
END;

CREATE TRIGGER validate_user_settings_collection_ownership_update
BEFORE UPDATE OF last_collection_id ON user_settings
WHEN NEW.last_collection_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: collection belongs to different user')
    WHERE (SELECT user_id FROM collections WHERE id = NEW.last_collection_id)
       != NEW.user_id;
END;

CREATE TRIGGER validate_prompt_tag_ownership_update BEFORE UPDATE ON prompt_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and tag belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_prompt_category_ownership_update BEFORE UPDATE ON prompt_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and category belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_prompt_collection_ownership_update BEFORE UPDATE ON prompt_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: prompt and collection belong to different users')
    WHERE (SELECT user_id FROM prompts WHERE id = NEW.prompt_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

CREATE TRIGGER validate_script_tag_ownership_update BEFORE UPDATE ON script_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and tag belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_script_category_ownership_update BEFORE UPDATE ON script_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and category belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_script_collection_ownership_update BEFORE UPDATE ON script_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: script and collection belong to different users')
    WHERE (SELECT user_id FROM scripts WHERE id = NEW.script_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

CREATE TRIGGER validate_chain_tag_ownership_update BEFORE UPDATE ON chain_tags
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and tag belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM tags WHERE id = NEW.tag_id);
END;

CREATE TRIGGER validate_chain_category_ownership_update BEFORE UPDATE ON chain_categories
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and category belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM categories WHERE id = NEW.category_id);
END;

CREATE TRIGGER validate_chain_collection_ownership_update BEFORE UPDATE ON chain_collections
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and collection belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM collections WHERE id = NEW.collection_id);
END;

CREATE TRIGGER validate_chain_step_prompt_ownership_update BEFORE UPDATE ON chain_steps
WHEN NEW.prompt_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and prompt belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM prompts WHERE id = NEW.prompt_id);
END;

CREATE TRIGGER validate_chain_step_script_ownership_update BEFORE UPDATE ON chain_steps
WHEN NEW.script_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'ownership mismatch: chain and script belong to different users')
    WHERE (SELECT user_id FROM chains WHERE id = NEW.chain_id)
       != (SELECT user_id FROM scripts WHERE id = NEW.script_id);
END;

-- ===========================================================================
-- FTS5 Virtual Tables and Synchronization Triggers
-- ===========================================================================

-- ---------------------------------------------------------------------------
-- Prompts FTS5
-- ---------------------------------------------------------------------------
CREATE VIRTUAL TABLE prompts_fts USING fts5(
    title,
    content,
    description,
    notes,
    content='prompts',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER prompts_fts_insert AFTER INSERT ON prompts
BEGIN
    INSERT INTO prompts_fts(rowid, title, content, description, notes)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.description, NEW.notes);
END;

CREATE TRIGGER prompts_fts_update AFTER UPDATE OF title, content, description, notes ON prompts
BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, title, content, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description, OLD.notes);
    INSERT INTO prompts_fts(rowid, title, content, description, notes)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.description, NEW.notes);
END;

CREATE TRIGGER prompts_fts_delete AFTER DELETE ON prompts
BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, title, content, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description, OLD.notes);
END;

-- ---------------------------------------------------------------------------
-- Scripts FTS5
-- ---------------------------------------------------------------------------
CREATE VIRTUAL TABLE scripts_fts USING fts5(
    title,
    content,
    description,
    notes,
    content='scripts',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER scripts_fts_insert AFTER INSERT ON scripts
BEGIN
    INSERT INTO scripts_fts(rowid, title, content, description, notes)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.description, NEW.notes);
END;

CREATE TRIGGER scripts_fts_update AFTER UPDATE OF title, content, description, notes ON scripts
BEGIN
    INSERT INTO scripts_fts(scripts_fts, rowid, title, content, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description, OLD.notes);
    INSERT INTO scripts_fts(rowid, title, content, description, notes)
    VALUES (NEW.id, NEW.title, NEW.content, NEW.description, NEW.notes);
END;

CREATE TRIGGER scripts_fts_delete AFTER DELETE ON scripts
BEGIN
    INSERT INTO scripts_fts(scripts_fts, rowid, title, content, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.content, OLD.description, OLD.notes);
END;

-- ---------------------------------------------------------------------------
-- Chains FTS5
-- ---------------------------------------------------------------------------
CREATE VIRTUAL TABLE chains_fts USING fts5(
    title,
    description,
    notes,
    content='chains',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER chains_fts_insert AFTER INSERT ON chains
BEGIN
    INSERT INTO chains_fts(rowid, title, description, notes)
    VALUES (NEW.id, NEW.title, NEW.description, NEW.notes);
END;

CREATE TRIGGER chains_fts_update AFTER UPDATE OF title, description, notes ON chains
BEGIN
    INSERT INTO chains_fts(chains_fts, rowid, title, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.description, OLD.notes);
    INSERT INTO chains_fts(rowid, title, description, notes)
    VALUES (NEW.id, NEW.title, NEW.description, NEW.notes);
END;

CREATE TRIGGER chains_fts_delete AFTER DELETE ON chains
BEGIN
    INSERT INTO chains_fts(chains_fts, rowid, title, description, notes)
    VALUES ('delete', OLD.id, OLD.title, OLD.description, OLD.notes);
END;
