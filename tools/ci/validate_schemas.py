#!/usr/bin/env python3
"""Schema-Documentation Consistency Validator for NeuronPrompter.

This script enforces bidirectional consistency between the database schema
documented in the LaTeX architecture document (docs/architecture/architecture.tex)
and the actual SQL DDL defined in the migration file (migrations/0001_initial.sql).
It parses schema tables, index definitions, trigger declarations, FTS5
configuration, and CHECK constraints from both sources, then performs eight
validation checks to detect discrepancies in either direction.

Exit codes:
    0 -- All checks pass (no errors, and no warnings unless --strict is active).
    1 -- At least one error-level discrepancy was found (or a warning under --strict).
    2 -- The script cannot parse the LaTeX file or the SQL migration file.

Invocation:
    python tools/ci/validate_schemas.py \\
        --tex docs/architecture/architecture.tex \\
        --sql migrations/0001_initial.sql \\
        [--strict] [--json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


# ---------------------------------------------------------------------------
# Data structures for parsed schema information from both sources
# ---------------------------------------------------------------------------

@dataclass
class ColumnSpec:
    """A column definition extracted from either LaTeX documentation or SQL DDL.

    Attributes:
        name: The column name (e.g., "user_id").
        sql_type: The normalized SQL type string (e.g., "TEXT NOT NULL").
        table: The table this column belongs to (e.g., "users").
    """
    name: str
    sql_type: str
    table: str


@dataclass
class IndexSpec:
    """An index definition extracted from LaTeX documentation or SQL DDL.

    Attributes:
        name: The index name (e.g., "idx_prompts_user").
        table: The table this index is defined on (e.g., "prompts").
        is_unique: Whether the index has the UNIQUE qualifier.
    """
    name: str
    table: str
    is_unique: bool


@dataclass
class TriggerSpec:
    """A trigger definition extracted from LaTeX documentation or SQL DDL.

    Attributes:
        name: The trigger name (e.g., "prompts_fts_insert").
        event: The trigger event (e.g., "AFTER INSERT").
        table: The table the trigger is defined on (e.g., "prompts").
    """
    name: str
    event: str
    table: str


@dataclass
class FTS5Spec:
    """An FTS5 virtual table configuration extracted from documentation or SQL.

    Attributes:
        table_name: The virtual table name (e.g., "prompts_fts").
        content_table: The backing content table (e.g., "prompts").
        content_rowid: The rowid column (e.g., "id").
        tokenizer: The tokenizer identifier (e.g., "unicode61").
        columns: The indexed column names.
    """
    table_name: str
    content_table: str
    content_rowid: str
    tokenizer: str
    columns: list[str]


@dataclass
class Diagnostic:
    """A single validation diagnostic (error or warning).

    Attributes:
        level: Either "error" or "warning".
        category: The check category identifier (e.g., "table_existence").
        table_name: The table this diagnostic applies to.
        detail: A human-readable description of the discrepancy.
    """
    level: str
    category: str
    table_name: str
    detail: str


# ---------------------------------------------------------------------------
# LaTeX text sanitization utilities
# ---------------------------------------------------------------------------

def sanitize_latex_name(raw: str) -> str:
    """Convert a LaTeX-formatted column or table name to a plain SQL identifier.

    Strips LaTeX-specific formatting: \\allowbreak commands, escaped underscores
    (\\_), \\raggedright, \\texttt{} wrappers, and leading/trailing whitespace.
    The resulting string is a plain identifier like "display_name".

    Args:
        raw: The raw name string extracted from a LaTeX table cell.

    Returns:
        A cleaned SQL identifier string.
    """
    cleaned = raw.replace("\\allowbreak", "")
    cleaned = cleaned.replace("\\_", "_")
    cleaned = cleaned.replace("\\raggedright", "")
    # Remove \texttt{...} wrappers, keeping inner content
    cleaned = re.sub(r"\\texttt\{([^}]*)\}", r"\1", cleaned)
    # Collapse all whitespace (including newlines from multi-line names)
    cleaned = re.sub(r"\s+", "", cleaned)
    return cleaned.strip()


def sanitize_latex_type(raw: str) -> str:
    """Normalize a SQL type string extracted from LaTeX documentation.

    Expands abbreviations used in tabularx tables (PK -> PRIMARY KEY,
    NN -> NOT NULL, FK -> FOREIGN KEY), strips LaTeX formatting commands,
    and normalizes whitespace. The result is an uppercase canonical type
    string suitable for direct comparison with normalized SQL DDL types.

    Args:
        raw: The raw type string from a LaTeX table cell.

    Returns:
        A normalized, uppercase SQL type string (e.g., "INTEGER PRIMARY KEY").
    """
    cleaned = raw.replace("\\raggedright", "").strip()
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    # Expand abbreviations used in tabularx tables
    cleaned = re.sub(r"\bPK\b", "PRIMARY KEY", cleaned)
    cleaned = re.sub(r"\bNN\b", "NOT NULL", cleaned)
    return cleaned.upper()


# ---------------------------------------------------------------------------
# LaTeX parsing routines
# ---------------------------------------------------------------------------

def _join_multiline_rows(table_body: str) -> list[str]:
    """Join LaTeX table rows spanning multiple source lines into logical rows.

    A LaTeX table row is terminated by \\\\ (double backslash). Column names
    with \\allowbreak and long descriptions can cause a single row to span
    multiple source lines. This function accumulates source lines until the
    row terminator is encountered, then emits the joined row as one string.

    LaTeX structural commands (\\toprule, \\midrule, \\bottomrule,
    \\endfirsthead, \\endhead, \\endfoot, \\normalfont, \\begin, \\end,
    \\multicolumn) are emitted as standalone entries without joining.

    Args:
        table_body: Raw LaTeX table body text between \\begin{} and \\caption{}.

    Returns:
        A list of joined logical row strings.
    """
    lines = table_body.split("\n")
    joined: list[str] = []
    current_parts: list[str] = []

    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue

        # Skip LaTeX comments (lines starting with %)
        if stripped.startswith("%"):
            continue

        # Strip inline LaTeX comments (% not preceded by \)
        stripped = re.sub(r"(?<!\\)%.*$", "", stripped).rstrip()
        if not stripped:
            continue

        # Structural LaTeX commands are standalone entries
        is_structural = (
            stripped.startswith("\\toprule")
            or stripped.startswith("\\midrule")
            or stripped.startswith("\\bottomrule")
            or stripped.startswith("\\endfirsthead")
            or stripped.startswith("\\endhead")
            or stripped.startswith("\\endfoot")
            or stripped.startswith("\\normalfont")
            or stripped.startswith("\\begin{")
            or stripped.startswith("\\end{")
            or stripped.startswith("\\multicolumn")
        )

        if is_structural:
            # Flush any accumulated parts before the structural command
            if current_parts:
                joined.append(" ".join(current_parts))
                current_parts = []
            joined.append(stripped)
            continue

        current_parts.append(stripped)

        # The LaTeX row terminator \\ (two backslashes) marks end of row
        if stripped.endswith("\\\\"):
            joined.append(" ".join(current_parts))
            current_parts = []

    # Flush remaining parts
    if current_parts:
        joined.append(" ".join(current_parts))

    return joined


def parse_schema_tables(tex_content: str) -> dict[str, list[ColumnSpec]]:
    """Extract column definitions from all formal schema tables in the LaTeX document.

    Finds all table environments whose caption matches
    "Schema: \\texttt{TABLE_NAME}" and extracts column name and SQL type
    from each data row. The rows use four columns:
        column_name & TYPE & CONSTRAINTS & Description

    The type and constraints cells are merged into a single normalized type
    string for comparison with the SQL DDL.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A dict mapping table name to a list of ColumnSpec instances.
    """
    tables: dict[str, list[ColumnSpec]] = {}

    # Find all schema table captions: "Schema: \texttt{TABLE_NAME}"
    caption_pattern = r"\\caption\{Schema:\s*\\texttt\{([^}]+)\}"
    caption_matches = list(re.finditer(caption_pattern, tex_content))

    for cap_match in caption_matches:
        table_name = sanitize_latex_name(cap_match.group(1))
        caption_pos = cap_match.start()

        # Search backward for the nearest \begin{table}, \begin{longtable},
        # or \begin{tabularx} that contains this caption
        table_start_pattern = r"\\begin\{(?:longtable|tabularx|table)\}"
        preceding = tex_content[:caption_pos]
        table_starts = list(re.finditer(table_start_pattern, preceding))
        if not table_starts:
            continue
        table_start = table_starts[-1].start()

        # Find the end of the table environment after the caption
        table_end_pattern = r"\\end\{(?:longtable|tabularx|table)\}"
        table_end_match = re.search(
            table_end_pattern, tex_content[table_start:]
        )
        table_end = (
            table_start + table_end_match.end()
            if table_end_match
            else len(tex_content)
        )

        # The table body is between the table start and the table end
        table_body = tex_content[table_start:table_end]

        # Join multi-line rows into single logical lines
        logical_rows = _join_multiline_rows(table_body)

        columns: list[ColumnSpec] = []

        for row in logical_rows:
            # Skip structural LaTeX commands and lines without column separators
            if row.startswith("\\") or "&" not in row:
                continue

            # Split into parts: name & type & constraints & description
            parts = row.split("&")
            if len(parts) < 3:
                continue

            col_name_raw = parts[0].strip()
            col_type_raw = parts[1].strip()

            # Skip the header row (Column & Type & ...)
            if "textbf" in col_name_raw or "textbf" in col_type_raw:
                continue

            col_name = sanitize_latex_name(col_name_raw)

            # Skip empty names (artifacts from non-data lines)
            if not col_name:
                continue

            # Merge type and constraints columns for the normalized type.
            # The LaTeX schema tables use 4 columns:
            #   Column & Type & Constraints & Description
            # We need to combine Type and Constraints for comparison.
            constraints_raw = parts[2].strip() if len(parts) >= 4 else ""
            col_type = _merge_latex_type_and_constraints(
                col_type_raw, constraints_raw
            )

            columns.append(ColumnSpec(
                name=col_name,
                sql_type=col_type,
                table=table_name,
            ))

        tables[table_name] = columns

    return tables


def _merge_latex_type_and_constraints(
    type_raw: str, constraints_raw: str,
) -> str:
    """Merge the Type and Constraints columns from a LaTeX schema table.

    The LaTeX schema tables split type information across two columns:
    - Type: base SQL type (e.g., "INTEGER", "TEXT")
    - Constraints: modifiers (e.g., "PK, autoincrement", "NOT NULL, UNIQUE")

    This function extracts the SQL-relevant parts from both and returns a
    normalized uppercase type string. Constraint tokens like "autoincrement",
    "FK ...", "default ...", "nullable", and "CHECK(...)" are processed
    to match the normalized SQL DDL output.

    Args:
        type_raw: The raw Type cell from the LaTeX table.
        constraints_raw: The raw Constraints cell from the LaTeX table.

    Returns:
        A normalized, uppercase SQL type string.
    """
    base_type = sanitize_latex_type(type_raw)

    # Clean the constraints string
    constraints = constraints_raw.replace("\\_", "_")
    constraints = constraints.replace("\\allowbreak", "")
    constraints = re.sub(r"\\texttt\{[^}]*\}", "", constraints)
    constraints = re.sub(r"\$[^$]*\$", "", constraints)  # Remove math mode
    constraints = re.sub(r"\\\\$", "", constraints).strip()  # Remove row terminator

    # Split constraints by comma and process each token
    modifiers: list[str] = []
    if constraints:
        for token in constraints.split(","):
            token = token.strip().upper()
            if not token:
                continue

            # Skip descriptive-only tokens that are not part of the SQL type
            if token in ("NULLABLE",):
                continue
            if token.startswith("FK"):
                continue
            if token.startswith("DEFAULT"):
                continue
            if token.startswith("CHECK"):
                continue

            # Expand abbreviations
            token = re.sub(r"\bPK\b", "PRIMARY KEY", token)
            token = re.sub(r"\bNN\b", "NOT NULL", token)

            modifiers.append(token)

    # Build the complete type string
    parts = [base_type]
    for mod in modifiers:
        # Avoid duplication (e.g., if PK is already in the type)
        if mod not in base_type:
            parts.append(mod)

    result = " ".join(parts)
    # Normalize whitespace
    result = re.sub(r"\s+", " ", result).strip()
    return result


def parse_latex_indexes(tex_content: str) -> list[IndexSpec]:
    """Extract named index definitions from the LaTeX index table.

    Finds the "Secondary indexes" table (identified by its caption or label
    tab:indexes) and extracts index names from \\texttt{idx_...} entries
    in the first column of each row.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A list of IndexSpec instances for all documented indexes.
    """
    indexes: list[IndexSpec] = []
    seen_names: set[str] = set()

    # Find the indexes table by its label
    idx_table_match = re.search(
        r"\\label\{tab:indexes\}(.*?)\\end\{(?:tabularx|table|longtable)\}",
        tex_content,
        re.DOTALL,
    )
    if not idx_table_match:
        # Try finding by caption
        idx_table_match = re.search(
            r"\\caption\{Secondary indexes\}(.*?)\\end\{(?:tabularx|table|longtable)\}",
            tex_content,
            re.DOTALL,
        )

    if idx_table_match:
        table_body = idx_table_match.group(1)
        # Extract \texttt{idx_...} names from each row
        idx_pattern = r"\\texttt\{(idx[^}]+)\}"
        for match in re.finditer(idx_pattern, table_body):
            raw_name = match.group(1)
            idx_name = sanitize_latex_name(raw_name)

            if idx_name in seen_names:
                continue

            table = _infer_table_from_index_name(idx_name)
            indexes.append(IndexSpec(
                name=idx_name, table=table, is_unique=False,
            ))
            seen_names.add(idx_name)

    # Also check for CREATE INDEX in verbatim blocks
    verbatim_pattern = r"\\begin\{verbatim\}(.*?)\\end\{verbatim\}"
    for match in re.finditer(verbatim_pattern, tex_content, re.DOTALL):
        block = match.group(1)
        create_idx_pattern = (
            r"CREATE\s+(UNIQUE\s+)?INDEX\s+"
            r"(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+ON\s+(\w+)"
        )
        for idx_match in re.finditer(create_idx_pattern, block, re.IGNORECASE):
            is_unique = idx_match.group(1) is not None
            idx_name = idx_match.group(2)
            table = idx_match.group(3)

            if idx_name not in seen_names:
                indexes.append(IndexSpec(
                    name=idx_name, table=table, is_unique=is_unique,
                ))
                seen_names.add(idx_name)

    return indexes


def _infer_table_from_index_name(idx_name: str) -> str:
    """Infer the table name from an index name using NeuronPrompter naming conventions.

    Index names follow various patterns:
        idx_prompts_*          -> prompts
        idx_scripts_*          -> scripts
        idx_chains_*           -> chains
        idx_versions_*         -> prompt_versions
        idx_script_versions_*  -> script_versions
        idx_chain_steps_*      -> chain_steps
        idx_pt_*               -> prompt_tags
        idx_pc_*               -> prompt_collections
        idx_pcat_*             -> prompt_categories
        idx_st_*               -> script_tags
        idx_sc_*               -> script_collections
        idx_scat_*             -> script_categories
        idx_ct_*               -> chain_tags
        idx_cc_*               -> chain_collections
        idx_ccat_*             -> chain_categories
        idx_tags_*             -> tags
        idx_collections_*      -> collections
        idx_categories_*       -> categories

    Args:
        idx_name: The index name string (e.g., "idx_prompts_user").

    Returns:
        The inferred table name string.
    """
    # Ordered from most specific to least specific prefixes
    prefix_map = {
        "idx_script_versions_": "script_versions",
        "idx_chain_steps_": "chain_steps",
        "idx_prompts_": "prompts",
        "idx_scripts_": "scripts",
        "idx_chains_": "chains",
        "idx_versions_": "prompt_versions",
        "idx_tags_": "tags",
        "idx_collections_": "collections",
        "idx_categories_": "categories",
        "idx_pt_": "prompt_tags",
        "idx_pc_": "prompt_collections",
        "idx_pcat_": "prompt_categories",
        "idx_st_": "script_tags",
        "idx_sc_": "script_collections",
        "idx_scat_": "script_categories",
        "idx_ct_": "chain_tags",
        "idx_cc_": "chain_collections",
        "idx_ccat_": "chain_categories",
    }

    for prefix, table in prefix_map.items():
        if idx_name.startswith(prefix):
            return table

    # Fallback: use the second component of the underscore-separated name
    parts = idx_name.split("_")
    if len(parts) >= 2:
        return parts[1]
    return "unknown"


def parse_latex_triggers(tex_content: str) -> list[TriggerSpec]:
    """Extract trigger references from the LaTeX document.

    Searches for trigger names mentioned as \\texttt{trigger_name} entries
    in itemized lists or prose paragraphs. NeuronPrompter has two classes
    of triggers:

    1. Ownership validation triggers (validate_*_ownership,
       validate_*_ownership_update) that enforce cross-user data integrity
       on junction tables and chain_steps.
    2. FTS5 synchronization triggers (*_fts_insert, *_fts_update,
       *_fts_delete) that maintain full-text search indexes.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A list of TriggerSpec instances for all documented triggers.
    """
    triggers: list[TriggerSpec] = []
    seen_names: set[str] = set()

    # Pattern 1: Trigger names in \texttt{} blocks that look like trigger names
    # This catches both ownership triggers and FTS triggers documented anywhere
    # Note: underscores may be escaped as \_ in LaTeX
    trigger_name_pattern = r"\\texttt\{((?:validate[\\_]|[a-z]+[\\_]fts[\\_])[^}]+)\}"
    for match in re.finditer(trigger_name_pattern, tex_content):
        raw_name = match.group(1)
        name = sanitize_latex_name(raw_name)

        if name in seen_names:
            continue

        event = _infer_trigger_event(name)
        table = _infer_trigger_table(name)
        triggers.append(TriggerSpec(name=name, event=event, table=table))
        seen_names.add(name)

    # Pattern 2: Explicit trigger documentation in prose or verbatim blocks
    # "trigger_name --- AFTER INSERT on table_name" pattern
    trigger_prose_pattern = (
        r"\\texttt\{([^}]+)\}\s*---\s*"
        r"\\texttt\{((?:AFTER|BEFORE)[^}]+)\}\s+on\s+"
        r"\\texttt\{(\w+)\}"
    )
    for match in re.finditer(trigger_prose_pattern, tex_content, re.DOTALL):
        raw_name = match.group(1)
        raw_event = match.group(2)
        raw_table = match.group(3)

        name = sanitize_latex_name(raw_name)
        if name in seen_names:
            continue

        event = raw_event.replace("\\_", "_").strip().upper()
        event = re.sub(r"\s+", " ", event)
        table = raw_table.replace("\\_", "_")

        triggers.append(TriggerSpec(name=name, event=event, table=table))
        seen_names.add(name)

    # Pattern 3: CREATE TRIGGER in verbatim blocks
    verbatim_pattern = r"\\begin\{verbatim\}(.*?)\\end\{verbatim\}"
    for match in re.finditer(verbatim_pattern, tex_content, re.DOTALL):
        block = match.group(1)
        create_trigger_pattern = (
            r"CREATE\s+TRIGGER\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+"
            r"(AFTER|BEFORE)\s+(INSERT|DELETE|UPDATE(?:\s+OF\s+[\w,\s]+)?)"
            r"\s+ON\s+(\w+)"
        )
        for t_match in re.finditer(create_trigger_pattern, block, re.IGNORECASE):
            name = t_match.group(1)
            if name in seen_names:
                continue
            timing = t_match.group(2).upper()
            event_type = t_match.group(3).upper()
            table = t_match.group(4)
            triggers.append(TriggerSpec(
                name=name, event=f"{timing} {event_type}", table=table,
            ))
            seen_names.add(name)

    return triggers


def _infer_trigger_event(trigger_name: str) -> str:
    """Infer the trigger event from a trigger name.

    Args:
        trigger_name: The trigger name (e.g., "prompts_fts_insert").

    Returns:
        The inferred event string (e.g., "AFTER INSERT").
    """
    if trigger_name.endswith("_insert"):
        return "AFTER INSERT"
    if trigger_name.endswith("_update"):
        if "ownership" in trigger_name:
            return "BEFORE UPDATE"
        return "AFTER UPDATE"
    if trigger_name.endswith("_delete"):
        return "AFTER DELETE"
    if "ownership" in trigger_name:
        return "BEFORE INSERT"
    return "UNKNOWN"


def _infer_trigger_table(trigger_name: str) -> str:
    """Infer the target table from a trigger name.

    Args:
        trigger_name: The trigger name (e.g., "validate_prompt_tag_ownership").

    Returns:
        The inferred table name.
    """
    # FTS triggers: {entity}_fts_{event}
    fts_match = re.match(r"(\w+)_fts_(?:insert|update|delete)$", trigger_name)
    if fts_match:
        return fts_match.group(1)

    # Ownership triggers: validate_{entity}_{event}_ownership[_update]
    # e.g., validate_prompt_tag_ownership -> prompt_tags
    # e.g., validate_chain_step_prompt_ownership -> chain_steps
    # e.g., validate_user_settings_collection_ownership_insert -> user_settings
    ownership_match = re.match(
        r"validate_(.+?)_ownership(?:_update)?$", trigger_name,
    )
    if ownership_match:
        entity = ownership_match.group(1)
        # Map entity descriptors to table names
        entity_table_map = {
            "prompt_tag": "prompt_tags",
            "prompt_category": "prompt_categories",
            "prompt_collection": "prompt_collections",
            "script_tag": "script_tags",
            "script_category": "script_categories",
            "script_collection": "script_collections",
            "chain_tag": "chain_tags",
            "chain_category": "chain_categories",
            "chain_collection": "chain_collections",
            "chain_step_prompt": "chain_steps",
            "chain_step_script": "chain_steps",
            "user_settings_collection": "user_settings",
        }
        return entity_table_map.get(entity, entity)

    return "unknown"


def parse_latex_fts5(tex_content: str) -> list[FTS5Spec]:
    """Extract FTS5 virtual table configuration from the LaTeX document.

    Searches for prose descriptions of FTS5 tables, including mentions of
    the virtual table name (e.g., "prompts_fts"), content table, tokenizer,
    and indexed columns.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A list of FTS5Spec instances for all documented FTS5 tables.
    """
    configs: list[FTS5Spec] = []

    # Look for FTS5 virtual table references in the document.
    # The LaTeX may describe FTS5 tables in prose or in verbatim blocks.

    # Check for verbatim CREATE VIRTUAL TABLE blocks
    verbatim_pattern = r"\\begin\{verbatim\}(.*?)\\end\{verbatim\}"
    for match in re.finditer(verbatim_pattern, tex_content, re.DOTALL):
        block = match.group(1)
        fts5_pattern = (
            r"CREATE\s+VIRTUAL\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+"
            r"USING\s+fts5\((.*?)\)"
        )
        for fts_match in re.finditer(fts5_pattern, block, re.IGNORECASE | re.DOTALL):
            spec = _parse_fts5_body(fts_match.group(1), fts_match.group(2))
            if spec:
                configs.append(spec)

    # Check for prose descriptions of FTS5 tables.
    # Look for patterns like "FTS5 virtual table that indexes the
    # \texttt{title}, \texttt{content}, ... columns"
    # combined with "content='prompts'" or similar references.
    fts5_prose_pattern = (
        r"FTS5\s+virtual\s+table\s+(?:that\s+)?index(?:es)?\s+"
        r"(?:the\s+)?((?:\\texttt\{[^}]+\}[\s,]*(?:and\s+)?)+)\s*columns?"
        r".*?\\texttt\{(\w+)\}\s+(?:table|tokenizer)"
    )
    prose_match = re.search(fts5_prose_pattern, tex_content, re.DOTALL | re.IGNORECASE)
    if prose_match:
        # Extract column names from \texttt{} references
        col_names_raw = prose_match.group(1)
        col_pattern = r"\\texttt\{([^}]+)\}"
        cols = [
            sanitize_latex_name(m.group(1))
            for m in re.finditer(col_pattern, col_names_raw)
        ]

        # Try to find the tokenizer name
        tokenizer_match = re.search(
            r"\\texttt\{(\w+)\}\s+tokenizer", tex_content,
        )
        tokenizer = tokenizer_match.group(1) if tokenizer_match else ""

        # Try to find the table name from prose like "on the \texttt{prompts} table"
        content_table_match = re.search(
            r"on\s+the\s+\\texttt\{(\w+)\}\s+table", tex_content,
        )
        content_table = (
            content_table_match.group(1) if content_table_match else ""
        )

        # The prose typically describes the prompts FTS; detect if it applies
        # to specific entity names
        if cols and not configs:
            # This is a fallback for prose-only FTS descriptions
            pass

    # Check for FTS5 tables in a formal LaTeX table (e.g., caption "FTS5 virtual tables")
    # Each row has: virtual_table_name & source_table & indexed_columns & triggers
    fts5_table_pattern = r"\\caption\{FTS5 virtual tables\}"
    fts5_cap = re.search(fts5_table_pattern, tex_content, re.IGNORECASE)
    if fts5_cap:
        cap_pos = fts5_cap.start()
        # Find enclosing table
        tbl_starts = list(re.finditer(
            r"\\begin\{(?:longtable|tabularx|table)\}",
            tex_content[:cap_pos],
        ))
        if tbl_starts:
            tbl_start = tbl_starts[-1].start()
            tbl_end_m = re.search(
                r"\\end\{(?:longtable|tabularx|table)\}",
                tex_content[tbl_start:],
            )
            tbl_end = tbl_start + tbl_end_m.end() if tbl_end_m else len(tex_content)
            tbl_body = tex_content[tbl_start:tbl_end]

            # Parse rows: \texttt{VT_NAME} & \texttt{SOURCE} & cols & triggers
            # Names may use \_ for escaped underscores
            row_pattern = (
                r"\\texttt\{([a-z_\\]+_fts)\}\s*&\s*\\texttt\{([a-z_\\]+)\}\s*&\s*"
                r"([^&]*?)&"
            )
            for rm in re.finditer(row_pattern, tbl_body):
                vt_name = rm.group(1).replace("\\_", "_")
                source_table = rm.group(2).replace("\\_", "_")
                cols_raw = rm.group(3)
                # Extract column names
                col_names = [
                    c.strip().rstrip(",")
                    for c in cols_raw.split(",")
                    if c.strip()
                ]
                # Check if already found
                existing_names = {c.table_name for c in configs}
                if vt_name not in existing_names:
                    configs.append(FTS5Spec(
                        table_name=vt_name,
                        content_table=source_table,
                        content_rowid="id",
                        tokenizer="unicode61",
                        columns=col_names,
                    ))

    return configs


def parse_latex_check_constraints(tex_content: str) -> dict[str, list[str]]:
    """Extract CHECK constraint mentions from the LaTeX document.

    Finds references to CHECK constraints in schema table constraint columns
    and prose descriptions. Returns a mapping from table name to a list of
    CHECK constraint descriptions found in the documentation.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A dict mapping table name to a list of CHECK constraint descriptions.
    """
    constraints: dict[str, list[str]] = {}

    # Find CHECK mentions in schema table constraint columns
    # These appear as "CHECK(length $\leq$ 2)" or similar in the Constraints column
    caption_pattern = r"\\caption\{Schema:\s*\\texttt\{([^}]+)\}"
    for cap_match in re.finditer(caption_pattern, tex_content):
        table_name = sanitize_latex_name(cap_match.group(1))
        caption_pos = cap_match.start()

        # Find the enclosing table environment
        table_start_pattern = r"\\begin\{(?:longtable|tabularx|table)\}"
        preceding = tex_content[:caption_pos]
        table_starts = list(re.finditer(table_start_pattern, preceding))
        if not table_starts:
            continue
        table_start = table_starts[-1].start()

        table_end_pattern = r"\\end\{(?:longtable|tabularx|table)\}"
        table_end_match = re.search(
            table_end_pattern, tex_content[table_start:]
        )
        table_end = (
            table_start + table_end_match.end()
            if table_end_match
            else len(tex_content)
        )
        table_body = tex_content[table_start:table_end]

        # Find CHECK mentions in the table body
        check_pattern = r"CHECK\([^)]*\)"
        checks = re.findall(check_pattern, table_body, re.IGNORECASE)
        if checks:
            constraints[table_name] = [c.strip() for c in checks]

    return constraints


# ---------------------------------------------------------------------------
# SQL parsing routines (raw SQL, not Rust-embedded)
# ---------------------------------------------------------------------------

def parse_sql_tables(sql_content: str) -> dict[str, list[ColumnSpec]]:
    """Parse CREATE TABLE statements from raw SQL to extract column definitions.

    Searches the SQL migration file for CREATE TABLE patterns and extracts
    column definitions from the body. Handles multiline CREATE TABLE blocks
    that span many lines, nested parentheses from CHECK constraints, and
    COLLATE NOCASE in column definitions.

    Lines that define table-level constraints (UNIQUE, FOREIGN KEY, CHECK,
    CONSTRAINT, PRIMARY KEY as standalone) are skipped.

    Args:
        sql_content: The full SQL migration file content as a string.

    Returns:
        A dict mapping table name to a list of ColumnSpec instances.
    """
    tables: dict[str, list[ColumnSpec]] = {}

    # Match CREATE TABLE with optional IF NOT EXISTS
    table_pattern = r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s*\("
    for match in re.finditer(table_pattern, sql_content, re.IGNORECASE):
        table_name = match.group(1)
        body_start = match.end()

        # Find the matching closing parenthesis (handle nested parens
        # from CHECK constraints, REFERENCES, etc.)
        depth = 1
        pos = body_start
        while pos < len(sql_content) and depth > 0:
            if sql_content[pos] == "(":
                depth += 1
            elif sql_content[pos] == ")":
                depth -= 1
            pos += 1

        body = sql_content[body_start:pos - 1]
        columns = _parse_column_definitions(body, table_name)
        tables[table_name] = columns

    return tables


def _parse_column_definitions(body: str, table_name: str) -> list[ColumnSpec]:
    """Parse column definitions from a CREATE TABLE body string.

    Splits the body by top-level commas (not inside nested parentheses) and
    identifies column definition lines. Each column line has the format:
        column_name TYPE [NOT NULL] [DEFAULT ...] [REFERENCES ...]

    Lines starting with constraint keywords (UNIQUE, FOREIGN KEY, CHECK,
    CONSTRAINT, PRIMARY KEY) are skipped as they define table-level
    constraints rather than columns.

    Args:
        body: The text between the opening and closing parentheses of a
            CREATE TABLE statement.
        table_name: The table name for ColumnSpec attribution.

    Returns:
        A list of ColumnSpec instances, one per column.
    """
    columns: list[ColumnSpec] = []
    parts = _split_top_level_commas(body)

    for part in parts:
        line = part.strip()
        # Remove SQL comments (-- style)
        line = re.sub(r"--.*$", "", line, flags=re.MULTILINE)
        line = " ".join(line.split())

        if not line:
            continue

        # Skip table-level constraint definitions
        upper_line = line.upper().lstrip()
        if upper_line.startswith((
            "UNIQUE", "FOREIGN", "CHECK", "CONSTRAINT", "PRIMARY KEY",
        )):
            continue

        # Parse column: column_name followed by type and optional constraints
        col_match = re.match(r"(\w+)\s+(.+)", line)
        if col_match:
            col_name = col_match.group(1)
            col_type_raw = col_match.group(2).strip()
            col_type = _normalize_sql_type(col_type_raw)

            columns.append(ColumnSpec(
                name=col_name,
                sql_type=col_type,
                table=table_name,
            ))

    return columns


def _split_top_level_commas(text: str) -> list[str]:
    """Split text by commas that are not inside parentheses.

    Tracks parenthesis nesting depth to avoid splitting on commas inside
    CHECK(), REFERENCES(), or other SQL constructs that contain commas
    within parenthesized arguments.

    Args:
        text: The SQL text to split.

    Returns:
        A list of comma-separated segments.
    """
    parts: list[str] = []
    depth = 0
    current: list[str] = []

    for char in text:
        if char == "(":
            depth += 1
            current.append(char)
        elif char == ")":
            depth -= 1
            current.append(char)
        elif char == "," and depth == 0:
            parts.append("".join(current))
            current = []
        else:
            current.append(char)

    if current:
        parts.append("".join(current))

    return parts


def _normalize_sql_type(raw_type: str) -> str:
    """Normalize a SQL column type definition for cross-source comparison.

    Extracts the base type and key modifiers (PRIMARY KEY, NOT NULL,
    AUTOINCREMENT, UNIQUE) while stripping DEFAULT clauses, REFERENCES
    foreign key constraints, ON DELETE CASCADE specifications, CHECK
    constraints, and COLLATE NOCASE directives. These stripped elements
    are not relevant for schema validation because the LaTeX documentation
    describes them in the Constraints or Description columns rather than
    the Type column.

    Args:
        raw_type: The raw SQL type string from a column definition in a
            CREATE TABLE statement.

    Returns:
        A normalized, uppercase type string (e.g., "INTEGER PRIMARY KEY AUTOINCREMENT").
    """
    # Remove REFERENCES table(column) [ON DELETE CASCADE/SET NULL/RESTRICT] FK specs
    cleaned = re.sub(
        r"REFERENCES\s+\w+\([^)]*\)(\s+ON\s+DELETE\s+\w+(\s+\w+)?)?",
        "",
        raw_type,
        flags=re.IGNORECASE,
    )
    # Remove COLLATE NOCASE directives
    cleaned = re.sub(r"COLLATE\s+NOCASE", "", cleaned, flags=re.IGNORECASE)
    # Remove DEFAULT (expression) clauses with parenthesized expressions
    # Must handle nested parens: DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
    cleaned = _strip_default_clause(cleaned)
    # Remove DEFAULT 'string_value' clauses (including empty string DEFAULT '')
    cleaned = re.sub(r"DEFAULT\s+'[^']*'", "", cleaned, flags=re.IGNORECASE)
    # Remove DEFAULT numeric_value clauses (DEFAULT 0, DEFAULT -1)
    cleaned = re.sub(r"DEFAULT\s+-?\d+", "", cleaned, flags=re.IGNORECASE)
    # Remove inline CHECK constraints. CHECK constraints may contain nested
    # parentheses (e.g., CHECK(col IN ('a', 'b'))), so strip from CHECK to
    # end-of-string after removing nested parens.
    cleaned = re.sub(r"CHECK\b.*", "", cleaned, flags=re.IGNORECASE | re.DOTALL)
    # Collapse whitespace and uppercase for canonical comparison
    cleaned = re.sub(r"\s+", " ", cleaned).strip().upper()
    return cleaned


def _strip_default_clause(text: str) -> str:
    """Strip DEFAULT (expression) clauses with proper parenthesis nesting.

    Handles expressions like DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
    where there are nested parentheses inside the DEFAULT value.

    Args:
        text: The SQL type string potentially containing a DEFAULT clause.

    Returns:
        The string with DEFAULT (expression) clauses removed.
    """
    result = text
    default_match = re.search(r"DEFAULT\s*\(", result, flags=re.IGNORECASE)
    while default_match:
        start = default_match.start()
        paren_start = default_match.end() - 1  # Position of the opening (
        depth = 1
        pos = paren_start + 1
        while pos < len(result) and depth > 0:
            if result[pos] == "(":
                depth += 1
            elif result[pos] == ")":
                depth -= 1
            pos += 1
        result = result[:start] + result[pos:]
        default_match = re.search(r"DEFAULT\s*\(", result, flags=re.IGNORECASE)
    return result


def parse_sql_indexes(sql_content: str) -> list[IndexSpec]:
    """Extract CREATE INDEX and CREATE UNIQUE INDEX statements from SQL.

    Searches for all CREATE [UNIQUE] INDEX patterns in the raw SQL content.

    Args:
        sql_content: The full SQL migration file content as a string.

    Returns:
        A list of IndexSpec instances for all named indexes in the DDL.
    """
    indexes: list[IndexSpec] = []

    idx_pattern = (
        r"CREATE\s+(UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+ON\s+(\w+)"
    )

    for match in re.finditer(idx_pattern, sql_content, re.IGNORECASE):
        is_unique = match.group(1) is not None
        idx_name = match.group(2)
        table = match.group(3)

        indexes.append(IndexSpec(
            name=idx_name,
            table=table,
            is_unique=is_unique,
        ))

    return indexes


def parse_sql_triggers(sql_content: str) -> list[TriggerSpec]:
    """Extract CREATE TRIGGER statements from raw SQL.

    Searches for all CREATE TRIGGER patterns in the SQL file. Each trigger
    specifies its timing (AFTER/BEFORE), event type (INSERT/DELETE/UPDATE
    OF column[,column...]), and target table. Handles optional WHEN clauses
    between the ON clause and the BEGIN block.

    Args:
        sql_content: The full SQL migration file content as a string.

    Returns:
        A list of TriggerSpec instances for all triggers in the DDL.
    """
    triggers: list[TriggerSpec] = []

    # Pattern handles:
    # CREATE TRIGGER name BEFORE INSERT ON table
    # CREATE TRIGGER name AFTER UPDATE OF col1, col2 ON table
    # CREATE TRIGGER name BEFORE UPDATE ON table
    # CREATE TRIGGER name BEFORE INSERT ON table WHEN condition
    trigger_pattern = (
        r"CREATE\s+TRIGGER\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+"
        r"(AFTER|BEFORE)\s+"
        r"(INSERT|DELETE|UPDATE(?:\s+OF\s+[\w,\s]+?)?)\s+"
        r"ON\s+(\w+)"
    )

    for match in re.finditer(trigger_pattern, sql_content, re.IGNORECASE):
        name = match.group(1)
        timing = match.group(2).upper()
        event = match.group(3).strip().upper()
        # Normalize whitespace in UPDATE OF column lists
        event = re.sub(r"\s+", " ", event)
        table = match.group(4)

        triggers.append(TriggerSpec(
            name=name,
            event=f"{timing} {event}",
            table=table,
        ))

    return triggers


def parse_sql_fts5(sql_content: str) -> list[FTS5Spec]:
    """Extract FTS5 virtual table configurations from raw SQL.

    Parses all CREATE VIRTUAL TABLE ... USING fts5(...) statements to
    extract the virtual table name, indexed columns, backing content table,
    content_rowid, and tokenizer.

    Args:
        sql_content: The full SQL migration file content as a string.

    Returns:
        A list of FTS5Spec instances for all FTS5 virtual tables.
    """
    configs: list[FTS5Spec] = []

    # The fts5 body may span multiple lines; use DOTALL
    fts5_pattern = (
        r"CREATE\s+VIRTUAL\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s+"
        r"USING\s+fts5\((.*?)\)\s*;"
    )

    for match in re.finditer(fts5_pattern, sql_content, re.IGNORECASE | re.DOTALL):
        table_name = match.group(1)
        fts5_body = match.group(2)

        spec = _parse_fts5_body(table_name, fts5_body)
        if spec:
            configs.append(spec)

    return configs


def _parse_fts5_body(table_name: str, fts5_body: str) -> Optional[FTS5Spec]:
    """Parse the body of an fts5() declaration.

    Extracts indexed columns, content= parameter, content_rowid= parameter,
    and tokenize= parameter from the FTS5 declaration body.

    Args:
        table_name: The virtual table name.
        fts5_body: The text inside fts5(...).

    Returns:
        An FTS5Spec instance, or None if parsing fails.
    """
    columns: list[str] = []
    content_table = ""
    content_rowid = ""
    tokenizer = ""

    # Split by comma at top level (respecting quotes)
    parts = [p.strip() for p in fts5_body.split(",")]

    for part in parts:
        part = part.strip()
        if not part:
            continue

        # Check for content= parameter
        content_match = re.match(r"content\s*=\s*'?(\w+)'?", part)
        if content_match:
            content_table = content_match.group(1)
            continue

        # Check for content_rowid= parameter
        rowid_match = re.match(r"content_rowid\s*=\s*'?(\w+)'?", part)
        if rowid_match:
            content_rowid = rowid_match.group(1)
            continue

        # Check for tokenize= parameter
        tokenize_match = re.match(r"tokenize\s*=\s*'([^']+)'", part)
        if tokenize_match:
            tokenizer = tokenize_match.group(1).split()[0]  # First word is tokenizer name
            continue

        # Otherwise it is an indexed column name
        col_name = part.strip().strip("'\"")
        if re.match(r"^\w+$", col_name):
            columns.append(col_name)

    return FTS5Spec(
        table_name=table_name,
        content_table=content_table,
        content_rowid=content_rowid,
        tokenizer=tokenizer,
        columns=columns,
    )


def parse_sql_check_constraints(sql_content: str) -> dict[str, list[str]]:
    """Extract CHECK constraints from all CREATE TABLE statements in SQL.

    Finds inline CHECK constraints within column definitions and table-level
    CHECK constraints. Returns a mapping from table name to a list of CHECK
    constraint expressions.

    Args:
        sql_content: The full SQL migration file content as a string.

    Returns:
        A dict mapping table name to a list of CHECK constraint expressions.
    """
    constraints: dict[str, list[str]] = {}

    # Parse each CREATE TABLE to find CHECK constraints
    table_pattern = r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s*\("
    for match in re.finditer(table_pattern, sql_content, re.IGNORECASE):
        table_name = match.group(1)
        body_start = match.end()

        # Find matching closing paren
        depth = 1
        pos = body_start
        while pos < len(sql_content) and depth > 0:
            if sql_content[pos] == "(":
                depth += 1
            elif sql_content[pos] == ")":
                depth -= 1
            pos += 1

        body = sql_content[body_start:pos - 1]

        # Find all CHECK(...) expressions with proper nesting
        checks: list[str] = []
        check_positions = [
            m.start()
            for m in re.finditer(r"\bCHECK\s*\(", body, re.IGNORECASE)
        ]
        for check_pos in check_positions:
            # Find the opening paren after CHECK
            paren_pos = body.index("(", check_pos)
            check_depth = 1
            end_pos = paren_pos + 1
            while end_pos < len(body) and check_depth > 0:
                if body[end_pos] == "(":
                    check_depth += 1
                elif body[end_pos] == ")":
                    check_depth -= 1
                end_pos += 1
            check_expr = body[check_pos:end_pos].strip()
            checks.append(check_expr)

        if checks:
            constraints[table_name] = checks

    return constraints


# ---------------------------------------------------------------------------
# Validation check implementations
# ---------------------------------------------------------------------------

def _extract_mentioned_tables(tex_content: str) -> set[str]:
    """Find table names mentioned via \\texttt{TABLE_NAME} in the document.

    This supplements formal schema tables by catching junction tables and
    other tables that are documented in prose rather than in formal schema
    table environments.  Handles both plain underscores and LaTeX-escaped
    underscores (\\_).
    """
    mentioned: set[str] = set()
    # Match \texttt{...} with either plain or escaped underscores
    for m in re.finditer(r"\\texttt\{([a-z][a-z0-9_\\]+)\}", tex_content):
        raw = m.group(1)
        # Normalize LaTeX-escaped underscores to plain underscores
        name = raw.replace("\\_", "_")
        # Only include names that look like table identifiers
        if "_" in name and "/" not in name and "::" not in name:
            mentioned.add(name)
    return mentioned


def check_table_existence(
    latex_tables: dict[str, list[ColumnSpec]],
    sql_tables: dict[str, list[ColumnSpec]],
    tex_content: str = "",
) -> list[Diagnostic]:
    """Check 1: Table existence mismatch (error severity).

    Verifies that every table with a formal schema table in the LaTeX document
    has a corresponding CREATE TABLE in the SQL migration, and vice versa.
    Tables mentioned via \\texttt{} in prose (e.g., junction tables) are also
    considered as documented.
    """
    diagnostics: list[Diagnostic] = []

    latex_names = set(latex_tables.keys())
    sql_names = set(sql_tables.keys())

    # Also consider tables mentioned in prose (e.g., junction tables)
    mentioned_tables = _extract_mentioned_tables(tex_content) if tex_content else set()
    # Only count mentioned tables that actually exist in SQL
    documented_names = latex_names | (mentioned_tables & sql_names)

    for name in sorted(latex_names - sql_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="table_existence",
            table_name=name,
            detail=(
                "table documented in architecture.tex but no "
                "CREATE TABLE in 0001_initial.sql"
            ),
        ))

    for name in sorted(sql_names - documented_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="table_existence",
            table_name=name,
            detail=(
                "table has CREATE TABLE in 0001_initial.sql but no formal "
                "schema table in architecture.tex"
            ),
        ))

    return diagnostics


def check_columns_missing_in_code(
    latex_tables: dict[str, list[ColumnSpec]],
    sql_tables: dict[str, list[ColumnSpec]],
) -> list[Diagnostic]:
    """Check 2: Column documented but missing in code (error severity).

    For each table that has a formal schema table in both the LaTeX document
    and the SQL DDL, verifies that every column listed in the documentation
    exists in the CREATE TABLE statement.
    """
    diagnostics: list[Diagnostic] = []

    for table_name in sorted(set(latex_tables.keys()) & set(sql_tables.keys())):
        latex_col_names = {c.name for c in latex_tables[table_name]}
        sql_col_names = {c.name for c in sql_tables[table_name]}

        for col in sorted(latex_col_names - sql_col_names):
            diagnostics.append(Diagnostic(
                level="error",
                category="column_missing_in_code",
                table_name=table_name,
                detail=(
                    f"column '{col}' documented in architecture.tex "
                    f"but absent from CREATE TABLE in 0001_initial.sql"
                ),
            ))

    return diagnostics


def check_columns_missing_in_docs(
    latex_tables: dict[str, list[ColumnSpec]],
    sql_tables: dict[str, list[ColumnSpec]],
) -> list[Diagnostic]:
    """Check 3: Column in code but missing from documentation (error severity).

    For each table that has a formal schema table in both sources, verifies
    that every column in the CREATE TABLE statement is documented in the
    LaTeX schema table.
    """
    diagnostics: list[Diagnostic] = []

    for table_name in sorted(set(latex_tables.keys()) & set(sql_tables.keys())):
        latex_col_names = {c.name for c in latex_tables[table_name]}
        sql_col_names = {c.name for c in sql_tables[table_name]}

        for col in sorted(sql_col_names - latex_col_names):
            diagnostics.append(Diagnostic(
                level="error",
                category="column_missing_in_docs",
                table_name=table_name,
                detail=(
                    f"column '{col}' in CREATE TABLE (0001_initial.sql) "
                    f"but absent from architecture.tex"
                ),
            ))

    return diagnostics


def check_column_type_mismatch(
    latex_tables: dict[str, list[ColumnSpec]],
    sql_tables: dict[str, list[ColumnSpec]],
) -> list[Diagnostic]:
    """Check 4: Column type mismatch (error severity).

    For columns that exist in both the LaTeX documentation and the SQL DDL,
    verifies that the SQL type string matches after normalization. Both sides
    are uppercased, abbreviations expanded (PK -> PRIMARY KEY, NN -> NOT NULL),
    and non-type clauses (DEFAULT, REFERENCES, CHECK, COLLATE) stripped
    before comparison.
    """
    diagnostics: list[Diagnostic] = []

    for table_name in sorted(set(latex_tables.keys()) & set(sql_tables.keys())):
        latex_by_name = {c.name: c for c in latex_tables[table_name]}
        sql_by_name = {c.name: c for c in sql_tables[table_name]}

        common_cols = set(latex_by_name.keys()) & set(sql_by_name.keys())

        for col_name in sorted(common_cols):
            latex_type = latex_by_name[col_name].sql_type
            sql_type = sql_by_name[col_name].sql_type

            if latex_type != sql_type:
                diagnostics.append(Diagnostic(
                    level="error",
                    category="column_type_mismatch",
                    table_name=table_name,
                    detail=(
                        f"column '{col_name}' type mismatch: "
                        f"docs='{latex_type}' sql='{sql_type}'"
                    ),
                ))

    return diagnostics


def check_index_existence(
    latex_indexes: list[IndexSpec],
    sql_indexes: list[IndexSpec],
) -> list[Diagnostic]:
    """Check 5: Index existence mismatch (error severity).

    Verifies that every named index documented in the LaTeX document exists
    in the SQL migration, and vice versa. Only explicitly named indexes
    (CREATE INDEX statements) are compared.
    """
    diagnostics: list[Diagnostic] = []

    latex_names = {idx.name for idx in latex_indexes}
    sql_names = {idx.name for idx in sql_indexes}

    for name in sorted(latex_names - sql_names):
        table = next(
            (i.table for i in latex_indexes if i.name == name), "unknown",
        )
        diagnostics.append(Diagnostic(
            level="error",
            category="index_existence",
            table_name=table,
            detail=(
                f"index '{name}' documented in architecture.tex "
                f"but absent from 0001_initial.sql"
            ),
        ))

    for name in sorted(sql_names - latex_names):
        table = next(
            (i.table for i in sql_indexes if i.name == name), "unknown",
        )
        diagnostics.append(Diagnostic(
            level="error",
            category="index_existence",
            table_name=table,
            detail=(
                f"index '{name}' in 0001_initial.sql but not documented "
                f"in architecture.tex"
            ),
        ))

    return diagnostics


def check_trigger_existence(
    latex_triggers: list[TriggerSpec],
    sql_triggers: list[TriggerSpec],
) -> list[Diagnostic]:
    """Check 6: Trigger existence mismatch (warning severity).

    Verifies that every trigger in the SQL migration is documented in the
    LaTeX document, and vice versa. Includes both ownership validation
    triggers and FTS5 synchronization triggers.
    """
    diagnostics: list[Diagnostic] = []

    latex_names = {t.name for t in latex_triggers}
    sql_names = {t.name for t in sql_triggers}

    for name in sorted(latex_names - sql_names):
        table = next(
            (t.table for t in latex_triggers if t.name == name), "unknown",
        )
        diagnostics.append(Diagnostic(
            level="warning",
            category="trigger_existence",
            table_name=table,
            detail=(
                f"trigger '{name}' documented in architecture.tex "
                f"but absent from 0001_initial.sql"
            ),
        ))

    for name in sorted(sql_names - latex_names):
        table = next(
            (t.table for t in sql_triggers if t.name == name), "unknown",
        )
        diagnostics.append(Diagnostic(
            level="warning",
            category="trigger_existence",
            table_name=table,
            detail=(
                f"trigger '{name}' in 0001_initial.sql but not documented "
                f"in architecture.tex"
            ),
        ))

    return diagnostics


def check_fts5_config(
    latex_fts5: list[FTS5Spec],
    sql_fts5: list[FTS5Spec],
) -> list[Diagnostic]:
    """Check 7: FTS5 configuration mismatch (error severity).

    Compares FTS5 virtual table definitions between the LaTeX documentation
    and the SQL DDL. Checks table names, content tables, tokenizers, and
    indexed columns.
    """
    diagnostics: list[Diagnostic] = []

    latex_by_name = {f.table_name: f for f in latex_fts5}
    sql_by_name = {f.table_name: f for f in sql_fts5}

    latex_names = set(latex_by_name.keys())
    sql_names = set(sql_by_name.keys())

    for name in sorted(latex_names - sql_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="fts5_config",
            table_name=name,
            detail=(
                f"FTS5 virtual table '{name}' documented in architecture.tex "
                f"but absent from 0001_initial.sql"
            ),
        ))

    for name in sorted(sql_names - latex_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="fts5_config",
            table_name=name,
            detail=(
                f"FTS5 virtual table '{name}' in 0001_initial.sql "
                f"but not documented in architecture.tex"
            ),
        ))

    # Compare matching FTS5 configs
    for name in sorted(latex_names & sql_names):
        latex_spec = latex_by_name[name]
        sql_spec = sql_by_name[name]

        if latex_spec.content_table != sql_spec.content_table:
            diagnostics.append(Diagnostic(
                level="error",
                category="fts5_config",
                table_name=name,
                detail=(
                    f"FTS5 content table mismatch: "
                    f"docs='{latex_spec.content_table}' "
                    f"sql='{sql_spec.content_table}'"
                ),
            ))

        if latex_spec.tokenizer != sql_spec.tokenizer:
            diagnostics.append(Diagnostic(
                level="error",
                category="fts5_config",
                table_name=name,
                detail=(
                    f"FTS5 tokenizer mismatch: "
                    f"docs='{latex_spec.tokenizer}' "
                    f"sql='{sql_spec.tokenizer}'"
                ),
            ))

        latex_cols = sorted(latex_spec.columns)
        sql_cols = sorted(sql_spec.columns)
        if latex_cols != sql_cols:
            diagnostics.append(Diagnostic(
                level="error",
                category="fts5_config",
                table_name=name,
                detail=(
                    f"FTS5 indexed columns mismatch: "
                    f"docs={latex_cols} sql={sql_cols}"
                ),
            ))

    return diagnostics


def check_check_constraints(
    latex_checks: dict[str, list[str]],
    sql_checks: dict[str, list[str]],
) -> list[Diagnostic]:
    """Check 8: CHECK constraint documentation coverage (warning severity).

    Verifies that tables with CHECK constraints in SQL have at least some
    CHECK constraint documentation in the LaTeX document. This is a coverage
    check rather than an exact match, because LaTeX may describe CHECK
    constraints in prose or abbreviated form.
    """
    diagnostics: list[Diagnostic] = []

    for table_name in sorted(sql_checks.keys()):
        sql_count = len(sql_checks[table_name])
        latex_count = len(latex_checks.get(table_name, []))

        if latex_count == 0 and sql_count > 0:
            diagnostics.append(Diagnostic(
                level="warning",
                category="check_constraints",
                table_name=table_name,
                detail=(
                    f"SQL defines {sql_count} CHECK constraint(s) but "
                    f"architecture.tex does not mention CHECK for this table"
                ),
            ))

    return diagnostics


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------

def format_human_readable(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as human-readable text lines.

    Each line has the format:
        [ERROR|WARN] <category>: <table_name> -- <detail>

    A summary line at the end reports total error and warning counts.
    """
    lines: list[str] = []
    error_count = 0
    warning_count = 0

    for diag in diagnostics:
        if diag.level == "error":
            prefix = "[ERROR]"
            error_count += 1
        else:
            prefix = "[WARN]"
            warning_count += 1

        lines.append(
            f"{prefix} {diag.category}: {diag.table_name} -- {diag.detail}"
        )

    lines.append(f"SUMMARY: {error_count} error(s), {warning_count} warning(s)")
    return "\n".join(lines)


def format_json(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as a JSON string.

    The JSON object has three keys: "errors" (list of error entries),
    "warnings" (list of warning entries), and "summary" (counts).
    """
    errors = []
    warnings = []

    for diag in diagnostics:
        entry = {
            "category": diag.category,
            "table": diag.table_name,
            "detail": diag.detail,
        }
        if diag.level == "error":
            errors.append(entry)
        else:
            warnings.append(entry)

    result = {
        "errors": errors,
        "warnings": warnings,
        "summary": {
            "error_count": len(errors),
            "warning_count": len(warnings),
        },
    }

    return json.dumps(result, indent=2)


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def run_validation(tex_path: Path, sql_path: Path) -> list[Diagnostic]:
    """Execute all eight validation checks and return the combined diagnostics.

    Parses the LaTeX document for formal schema table definitions, index
    references, trigger documentation, FTS5 configuration, and CHECK
    constraint mentions. Parses the SQL migration for DDL statements.
    Then runs each check to detect discrepancies between the two sources.

    Args:
        tex_path: Path to the LaTeX architecture document.
        sql_path: Path to the SQL migration file.

    Returns:
        A list of Diagnostic instances from all eight checks.
    """
    tex_content = tex_path.read_text(encoding="utf-8")
    sql_content = sql_path.read_text(encoding="utf-8")

    # Parse LaTeX documentation
    latex_tables = parse_schema_tables(tex_content)
    latex_indexes = parse_latex_indexes(tex_content)
    latex_triggers = parse_latex_triggers(tex_content)
    latex_fts5 = parse_latex_fts5(tex_content)
    latex_checks = parse_latex_check_constraints(tex_content)

    # Parse SQL DDL from migration file
    sql_tables = parse_sql_tables(sql_content)
    sql_indexes = parse_sql_indexes(sql_content)
    sql_triggers = parse_sql_triggers(sql_content)
    sql_fts5 = parse_sql_fts5(sql_content)
    sql_checks = parse_sql_check_constraints(sql_content)

    # Run all eight validation checks
    all_diagnostics: list[Diagnostic] = []

    # Check 1: table_existence (error)
    all_diagnostics.extend(check_table_existence(latex_tables, sql_tables, tex_content))
    # Check 2: column_missing_in_code (error)
    all_diagnostics.extend(
        check_columns_missing_in_code(latex_tables, sql_tables),
    )
    # Check 3: column_missing_in_docs (error)
    all_diagnostics.extend(
        check_columns_missing_in_docs(latex_tables, sql_tables),
    )
    # Check 4: column_type_mismatch (error)
    all_diagnostics.extend(
        check_column_type_mismatch(latex_tables, sql_tables),
    )
    # Check 5: index_existence (error)
    all_diagnostics.extend(check_index_existence(latex_indexes, sql_indexes))
    # Check 6: trigger_existence (warning)
    all_diagnostics.extend(
        check_trigger_existence(latex_triggers, sql_triggers),
    )
    # Check 7: fts5_config (error)
    all_diagnostics.extend(check_fts5_config(latex_fts5, sql_fts5))
    # Check 8: check_constraints (warning)
    all_diagnostics.extend(check_check_constraints(latex_checks, sql_checks))

    return all_diagnostics


def main() -> int:
    """Parse command-line arguments, run validation, and produce output.

    Returns:
        Exit code: 0 if all checks pass, 1 if discrepancies found, 2 if
        the script cannot parse the input files.
    """
    parser = argparse.ArgumentParser(
        description=(
            "Schema-Documentation Consistency Validator for NeuronPrompter. "
            "Enforces bidirectional consistency between the LaTeX architecture "
            "document and the SQL DDL in 0001_initial.sql."
        ),
    )
    parser.add_argument(
        "--tex",
        required=True,
        type=Path,
        help=(
            "Path to the LaTeX architecture document "
            "(e.g., docs/architecture/architecture.tex)"
        ),
    )
    parser.add_argument(
        "--sql",
        required=True,
        type=Path,
        help=(
            "Path to the SQL migration file "
            "(e.g., migrations/0001_initial.sql)"
        ),
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help=(
            "Treat warnings as errors "
            "(used in CI to block the pipeline on any discrepancy)"
        ),
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON instead of human-readable text",
    )
    args = parser.parse_args()

    tex_path: Path = args.tex.resolve()
    sql_path: Path = args.sql.resolve()

    if not tex_path.is_file():
        print(
            f"Error: LaTeX file not found: {tex_path}",
            file=sys.stderr,
        )
        return 2

    if not sql_path.is_file():
        print(
            f"Error: SQL migration file not found: {sql_path}",
            file=sys.stderr,
        )
        return 2

    try:
        diagnostics = run_validation(tex_path, sql_path)
    except Exception as exc:
        print(f"Error: failed to run validation: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(format_json(diagnostics))
    else:
        if diagnostics:
            print(format_human_readable(diagnostics), file=sys.stderr)
        else:
            print("All checks passed.", file=sys.stderr)

    error_count = sum(1 for d in diagnostics if d.level == "error")
    warning_count = sum(1 for d in diagnostics if d.level == "warning")

    if error_count > 0:
        return 1
    if args.strict and warning_count > 0:
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
