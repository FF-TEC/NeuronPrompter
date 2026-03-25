#!/usr/bin/env python3
"""Architecture-Code Consistency Validator for NeuronPrompter.

This script enforces bidirectional consistency between the LaTeX architecture
document (docs/architecture/architecture.tex) and the Rust source tree.  It
parses tables from the LaTeX file (crate overview, module structure, and
feature flags), scans the Cargo workspace for actual crate directories,
source files, mod declarations, and feature definitions, then performs seven
validation checks to detect discrepancies in either direction.

Exit codes:
    0 -- All checks pass (no errors, and no warnings unless --strict is active).
    1 -- At least one error-level discrepancy was found (or a warning under --strict).
    2 -- The script cannot parse the LaTeX file or locate the workspace root.

Invocation:
    python tools/ci/validate_architecture.py \\
        --tex docs/architecture/architecture.tex \\
        --root . [--strict] [--json]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


# ---------------------------------------------------------------------------
# Data structures for parsed LaTeX content and Rust source tree information
# ---------------------------------------------------------------------------

@dataclass
class CrateSpec:
    """A crate entry extracted from the LaTeX crate overview table.

    Attributes:
        name: The crate name as it appears in the workspace
              (e.g., "neuronprompter-core").
        crate_type: Either "Library" or "Binary", indicating the crate category.
    """
    name: str
    crate_type: str


@dataclass
class ModuleEntry:
    """A module entry extracted from a LaTeX module structure table.

    Attributes:
        crate_name: The crate this module belongs to
                    (e.g., "neuronprompter-core").
        module_path: The module path from the table (e.g., "domain::user").
        file_name: The source filename from the table (e.g., "user.rs").
        file_path: The resolved relative path under src/
                   (e.g., "src/domain/user.rs").
        visibility: Either "public" or "private", if documented.
        feature_flag: The Cargo feature flag gating this module, if any.
    """
    crate_name: str
    module_path: str
    file_name: str
    file_path: str
    visibility: Optional[str] = None
    feature_flag: Optional[str] = None


@dataclass
class Diagnostic:
    """A single validation diagnostic (error or warning).

    Attributes:
        level: Either "error" or "warning".
        category: The check category identifier (e.g., "missing_source_files").
        crate_name: The crate this diagnostic applies to.
        detail: A human-readable description of the discrepancy.
    """
    level: str
    category: str
    crate_name: str
    detail: str


@dataclass
class RustCrateInfo:
    """Information gathered from scanning a Rust crate directory.

    Attributes:
        name: The crate name from its Cargo.toml.
        path: The absolute path to the crate directory.
        source_files: Set of relative paths (e.g., "src/lib.rs") for all
                      .rs files.
        mod_declarations: Dict mapping module name to its visibility
                          ("pub" or "priv").
        feature_gates: Dict mapping module name to the feature flag gating it.
        features: Set of feature flag names defined in [features] of
                  Cargo.toml.
        internal_deps: Set of workspace crate names this crate depends on.
    """
    name: str
    path: Path
    source_files: set[str] = field(default_factory=set)
    mod_declarations: dict[str, str] = field(default_factory=dict)
    feature_gates: dict[str, str] = field(default_factory=dict)
    features: set[str] = field(default_factory=set)
    internal_deps: set[str] = field(default_factory=set)


# ---------------------------------------------------------------------------
# LaTeX parsing routines
# ---------------------------------------------------------------------------

def sanitize_latex(raw: str) -> str:
    """Convert a LaTeX-formatted string to a plain-text equivalent.

    Strips LaTeX-specific escapes: ``\\allowbreak`` commands, escaped
    underscores (``\\_``), ``\\texttt{}`` wrappers, and leading/trailing
    whitespace.

    Args:
        raw: The raw string extracted from a LaTeX table cell.

    Returns:
        A cleaned plain-text string.
    """
    cleaned = raw.replace("\\allowbreak", "")
    cleaned = cleaned.replace("\\_", "_")
    # Strip \texttt{...} wrappers, keeping the contents
    cleaned = re.sub(r"\\texttt\{([^}]*)\}", r"\1", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned)
    return cleaned.strip()


def _extract_table_body_by_label(tex_content: str, label: str) -> Optional[str]:
    """Locate a LaTeX table/longtable by its ``\\label{...}`` and return its body.

    Searches backward from the label for the nearest ``\\begin{longtable}``
    or ``\\begin{table}`` and forward for the corresponding ``\\end{...}``.

    Args:
        tex_content: The full LaTeX document content.
        label: The label string (e.g., "tab:crate-overview").

    Returns:
        The table body text, or None if the label was not found.
    """
    label_pattern = re.escape(label)
    label_match = re.search(rf"\\label\{{{label_pattern}\}}", tex_content)
    if label_match is None:
        return None

    label_pos = label_match.start()

    # Search backward for table start
    table_start_pattern = r"\\begin\{(?:longtable|table|tabularx)\}"
    table_starts = list(re.finditer(table_start_pattern, tex_content[:label_pos]))
    if not table_starts:
        return None
    table_start = table_starts[-1].start()

    # Search forward for table end
    table_end_pattern = r"\\end\{(?:longtable|table|tabularx)\}"
    table_end_match = re.search(table_end_pattern, tex_content[table_start:])
    if table_end_match is None:
        return None
    table_end = table_start + table_end_match.end()

    return tex_content[table_start:table_end]


def _extract_table_body_by_caption(
    tex_content: str, caption_pattern: str,
) -> Optional[str]:
    """Locate a LaTeX table by its caption regex and return its body.

    Args:
        tex_content: The full LaTeX document content.
        caption_pattern: A regex pattern matching the caption text.

    Returns:
        The table body text, or None if the caption was not found.
    """
    caption_match = re.search(caption_pattern, tex_content)
    if caption_match is None:
        return None

    caption_pos = caption_match.start()

    table_start_pattern = r"\\begin\{(?:longtable|table|tabularx)\}"
    table_starts = list(re.finditer(table_start_pattern, tex_content[:caption_pos]))
    if not table_starts:
        # Caption may be after begin; try searching forward from position
        # a bit before the caption
        search_start = max(0, caption_pos - 2000)
        table_starts = list(
            re.finditer(table_start_pattern, tex_content[search_start:caption_pos])
        )
        if not table_starts:
            return None
        table_start = search_start + table_starts[-1].start()
    else:
        table_start = table_starts[-1].start()

    table_end_pattern = r"\\end\{(?:longtable|table|tabularx)\}"
    table_end_match = re.search(table_end_pattern, tex_content[table_start:])
    if table_end_match is None:
        return None
    table_end = table_start + table_end_match.end()

    return tex_content[table_start:table_end]


def parse_crate_overview(tex_content: str) -> list[CrateSpec]:
    """Extract crate definitions from the crate overview table.

    The table is identified by label ``tab:crate-overview`` and caption
    "Cargo workspace crates".  Rows have the format::

        neuronprompter-xxx & Library & description \\\\

    or with explicit ``\\texttt{}``::

        \\texttt{neuronprompter-xxx} & Library & description \\\\

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A list of CrateSpec instances, one per crate listed in the table.
    """
    crates: list[CrateSpec] = []

    # Try label first, fall back to caption
    table_body = _extract_table_body_by_label(tex_content, "tab:crate-overview")
    if table_body is None:
        table_body = _extract_table_body_by_caption(
            tex_content, r"\\caption\{Cargo workspace crates",
        )
    if table_body is None:
        return crates

    # Match rows: crate-name & Type & ...
    # Crate names may be wrapped in \texttt{} or use \_ for underscores
    row_pattern = (
        r"(?:\\texttt\{(neuronprompter[\w\\\_-]*)\}"
        r"|(neuronprompter[\w\\\_-]*))"
        r"\s*&\s*(Library|Binary)\s*&"
    )
    for match in re.finditer(row_pattern, table_body):
        raw_name = (match.group(1) or match.group(2)).strip()
        crate_name = sanitize_latex(raw_name)
        crate_type = match.group(3).strip()
        crates.append(CrateSpec(name=crate_name, crate_type=crate_type))

    return crates


def _resolve_file_path(module_path: str, file_name: str) -> str:
    """Resolve a module path and filename to a relative source file path.

    Given a module path like ``domain::user`` and filename ``user.rs``,
    produces ``src/domain/user.rs``.  For top-level entries with module path
    matching the filename stem (or empty/crate-root paths), produces
    ``src/<file_name>``.  The special file ``lib.rs`` always maps to
    ``src/lib.rs`` and ``main.rs`` to ``src/main.rs``.

    Args:
        module_path: The module path from the table (e.g., "domain::user").
        file_name: The source filename from the table (e.g., "user.rs").

    Returns:
        A relative path string like "src/domain/user.rs".
    """
    # If the file_name already contains a directory separator (/),
    # treat it as a relative path from src/
    if "/" in file_name:
        return f"src/{file_name}"

    # Entry points
    if file_name in ("lib.rs", "main.rs"):
        return f"src/{file_name}"

    # Convert module path separators to directory separators
    # domain::user -> domain/user
    path_parts = module_path.replace("::", "/")

    # If the filename is mod.rs, the path is src/<path_parts>/mod.rs
    if file_name == "mod.rs":
        return f"src/{path_parts}/mod.rs"

    # For a file like "user.rs" with module path "domain::user",
    # the directory is derived from the module path minus the last component
    # (since the last component corresponds to the filename)
    parts = path_parts.split("/")
    stem = file_name.rsplit(".", 1)[0]  # "user" from "user.rs"

    if len(parts) > 0 and parts[-1] == stem:
        # Last path component matches filename stem:
        # domain::user + user.rs -> src/domain/user.rs
        dir_parts = parts[:-1]
    elif len(parts) > 0 and parts[-1] != stem:
        # Last path component does NOT match filename stem:
        # domain + types.rs -> src/domain/types.rs
        dir_parts = parts
    else:
        dir_parts = []

    if dir_parts:
        return f"src/{'/'.join(dir_parts)}/{file_name}"
    return f"src/{file_name}"


def _join_multiline_latex(table_body: str) -> str:
    """Join lines that belong to the same LaTeX logical construct.

    In module structure tables, rows and ``\\multicolumn`` annotations can
    span multiple LaTeX source lines.  This function joins continuation
    lines with the preceding line, producing single logical lines for parsing.

    Args:
        table_body: Raw LaTeX table body text.

    Returns:
        The table body with multi-line constructs joined into single lines.
    """
    lines = table_body.split("\n")
    joined: list[str] = []

    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue

        is_new_entry = (
            stripped.startswith("\\midrule")
            or stripped.startswith("\\toprule")
            or stripped.startswith("\\bottomrule")
            or stripped.startswith("\\multicolumn")
            or stripped.startswith("\\endfirsthead")
            or stripped.startswith("\\endhead")
            or stripped.startswith("\\endfoot")
            or re.match(r"\\normalfont", stripped)
            # Module path entries (first column)
            or re.match(r"[a-zA-Z_]", stripped)
            # Entries starting with \texttt
            or stripped.startswith("\\texttt")
        )

        if is_new_entry or not joined:
            joined.append(stripped)
        else:
            joined[-1] = joined[-1] + " " + stripped

    return "\n".join(joined)


def parse_module_tables(tex_content: str) -> list[ModuleEntry]:
    """Extract module entries from per-crate module structure tables.

    Tables are identified by labels like ``tab:mod-core``, ``tab:mod-db``,
    ``tab:mod-application``, ``tab:mod-mcp``, ``tab:mod-app``.  Rows have
    the format::

        module\\_path & file\\_name.rs & Contents description \\\\

    where the module path uses ``::`` separators (LaTeX-escaped as ``\\_``
    or with ``\\allowbreak``), and the file column contains the source
    filename.

    Feature flag annotations appear as ``\\multicolumn`` rows containing
    "compiled under feature" or similar text.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A list of ModuleEntry instances for all modules across all crates.
    """
    modules: list[ModuleEntry] = []

    # Map of table label suffix -> crate name
    # We search for all module tables by looking for labels tab:mod-*
    label_pattern = r"\\label\{(tab:mod-[\w-]+)\}"
    label_matches = list(re.finditer(label_pattern, tex_content))

    # Also try caption-based detection for tables captioned with crate names
    caption_pattern = (
        r"\\caption\{.*?(?:module|Module).*?"
        r"(?:\\texttt\{(neuronprompter[\w\\\_-]*)\}|(neuronprompter[\w-]*))"
    )
    caption_matches = list(re.finditer(caption_pattern, tex_content))

    # Collect table bodies with their crate names
    table_bodies: list[tuple[str, str]] = []  # (crate_name, body)

    # Process label-based tables
    for label_match in label_matches:
        label = label_match.group(1)
        # Derive crate name from label: tab:mod-core -> neuronprompter-core
        # tab:mod-app -> neuronprompter (binary)
        # tab:mod-application -> neuronprompter-application
        suffix = label.replace("tab:mod-", "")
        if suffix == "app":
            crate_name = "neuronprompter"
        else:
            crate_name = f"neuronprompter-{suffix}"

        body = _extract_table_body_by_label(tex_content, label)
        if body is not None:
            table_bodies.append((crate_name, body))

    # Process caption-based tables (as fallback for tables without labels
    # or with labels we didn't catch)
    seen_crates = {name for name, _ in table_bodies}
    for cap_match in caption_matches:
        raw_name = (cap_match.group(1) or cap_match.group(2) or "").strip()
        crate_name = sanitize_latex(raw_name)
        if crate_name and crate_name not in seen_crates:
            body = _extract_table_body_by_caption(
                tex_content,
                re.escape(cap_match.group(0)),
            )
            if body is not None:
                table_bodies.append((crate_name, body))
                seen_crates.add(crate_name)

    # Feature flag section header pattern
    feature_section_pattern = (
        r"compiled under feature\s+\\texttt\{([^}]+)\}"
    )

    for crate_name, table_body in table_bodies:
        joined_body = _join_multiline_latex(table_body)
        all_lines = joined_body.split("\n")

        active_feature: Optional[str] = None
        post_header_pending: bool = False

        idx = 0
        while idx < len(all_lines):
            stripped = all_lines[idx].strip()
            idx += 1

            # Handle \midrule lines with feature gate state machine
            if stripped.startswith("\\midrule"):
                if post_header_pending:
                    post_header_pending = False
                    continue

                # Look ahead to determine if next meaningful line is a
                # feature \multicolumn
                lookahead = idx
                while lookahead < len(all_lines):
                    next_line = all_lines[lookahead].strip()
                    if next_line.startswith("\\midrule"):
                        lookahead += 1
                        continue
                    break
                else:
                    next_line = ""

                if not re.search(feature_section_pattern, next_line):
                    active_feature = None
                continue

            # Check for feature flag section header in \multicolumn lines
            feat_match = re.search(feature_section_pattern, stripped)
            if feat_match:
                active_feature = feat_match.group(1).strip()
                post_header_pending = True
                continue

            # Non-feature \multicolumn section headers reset feature
            if re.search(r"\\multicolumn", stripped):
                active_feature = None
                post_header_pending = False
                continue

            # Skip structural commands
            if stripped.startswith((
                "\\toprule", "\\bottomrule", "\\endfirsthead",
                "\\endhead", "\\endfoot", "\\normalfont",
                "\\begin{", "\\end{", "\\caption", "\\label",
                "Module", "\\textbf{Module",
            )):
                continue

            # Match module rows: module_path & file.rs & contents \\
            # The module path column uses :: separators, may have \_ escapes
            # The file column contains a filename like user.rs or lib.rs
            # File names may be wrapped in \texttt{...}, so .rs may be
            # followed by } before the & separator
            row_match = re.match(
                r"\s*([^&]+?)\s*&\s*([^&]*?\.rs\}?)\s*&\s*([^\\]*)",
                stripped,
            )
            if row_match:
                raw_mod_path = row_match.group(1)
                raw_file_name = row_match.group(2)

                module_path = sanitize_latex(raw_mod_path).strip()
                file_name = sanitize_latex(raw_file_name).strip()

                # Convert :: module path to directory path and resolve
                # the full file path
                file_path = _resolve_file_path(module_path, file_name)

                modules.append(ModuleEntry(
                    crate_name=crate_name,
                    module_path=module_path,
                    file_name=file_name,
                    file_path=file_path,
                    feature_flag=active_feature,
                ))

    return modules


def parse_feature_flags(tex_content: str) -> set[str]:
    """Extract documented feature flag names from the LaTeX document.

    Searches for a feature flag table (by caption containing "feature flag"
    or "Feature flag") and extracts ``\\texttt{<feature-name>}`` entries.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A set of feature flag name strings.
    """
    features: set[str] = set()

    # Try several caption patterns
    for pattern in [
        r"\\caption\{Feature flag definitions.*?\}",
        r"\\caption\{.*?[Ff]eature.*?flag.*?\}",
    ]:
        caption_match = re.search(pattern, tex_content)
        if caption_match is not None:
            break
    else:
        return features

    caption_pos = caption_match.start()
    table_start_pattern = r"\\begin\{(?:table|longtable|tabularx)\}"
    table_starts = list(re.finditer(table_start_pattern, tex_content[:caption_pos]))
    if not table_starts:
        return features

    table_start = table_starts[-1].start()
    table_end_pattern = r"\\end\{(?:table|longtable|tabularx)\}"
    table_end_match = re.search(table_end_pattern, tex_content[table_start:])
    if table_end_match is None:
        return features

    table_body = tex_content[table_start:table_start + table_end_match.end()]

    feature_pattern = r"\\texttt\{([\w-]+)\}"
    for match in re.finditer(feature_pattern, table_body):
        features.add(match.group(1))

    return features


def _transitive_closure(deps: dict[str, set[str]]) -> dict[str, set[str]]:
    """Compute the transitive closure of a dependency graph.

    For each crate, collects all crates reachable through any chain of
    dependency edges.  Uses iterative BFS per node.

    Args:
        deps: Direct dependency edges (crate -> set of dependency crates).

    Returns:
        A dict with the same keys, where each value is the full set of
        transitively reachable dependencies.
    """
    closure: dict[str, set[str]] = {}
    for crate in deps:
        reachable: set[str] = set()
        frontier = list(deps.get(crate, set()))
        while frontier:
            dep = frontier.pop()
            if dep not in reachable:
                reachable.add(dep)
                frontier.extend(deps.get(dep, set()) - reachable)
        closure[crate] = reachable
    return closure


@dataclass
class DependencyGraphInfo:
    """Parsed representation of the TikZ dependency graph.

    Separates the directly drawn edges from metadata flags so that the
    comparison logic can apply transitive closure and universal dependency
    suppression independently per comparison direction.
    """

    direct_edges: dict[str, set[str]]
    """Edges drawn as ``\\draw[dep]`` or ``\\draw[bindep]`` commands."""

    is_transitive_reduction: bool
    """True when the graph caption/body declares a transitive reduction layout."""

    universal_dep: str | None
    """Crate name declared as a dependency of all library crates via a text
    annotation band.  None when no such annotation exists."""

    library_crates: set[str]
    """Set of crate names represented as library crate nodes in the graph
    (excludes the binary entry point)."""


def parse_dependency_graph(tex_content: str) -> DependencyGraphInfo:
    """Extract the documented crate dependency edges from the TikZ dependency graph.

    The dependency graph uses TikZ ``\\draw[dep]`` commands to represent edges.
    Convention: ``\\draw[dep] (source) -- (target);`` means "source depends on
    target".

    Node labels in the graph use ``\\textbf{neuronprompter-xxx}\\\\...``
    format.  The node ID is the short identifier used in ``\\draw`` commands.

    Args:
        tex_content: The full LaTeX document content as a string.

    Returns:
        A DependencyGraphInfo with direct edges, layout flags, and annotation
        metadata.
    """
    deps: dict[str, set[str]] = {}

    # Locate the dependency graph TikZ picture.
    # There may be multiple tikzpicture environments (e.g., layer diagrams,
    # ER diagrams).  The dependency graph is the one that contains \draw[dep]
    # edge commands or the crate-deps label.
    graph_pattern = r"\\begin\{tikzpicture\}.*?\\end\{tikzpicture\}"
    graph_body = None
    for match in re.finditer(graph_pattern, tex_content, re.DOTALL):
        candidate = match.group(0)
        # Check if this tikzpicture contains dep-style edges or
        # is near the crate-deps label
        surrounding = tex_content[match.start():min(match.end() + 500, len(tex_content))]
        if r"\draw[dep]" in candidate or "crate-deps" in surrounding:
            graph_body = candidate
            graph_match = match
            break

    if graph_body is None:
        return DependencyGraphInfo(
            direct_edges={},
            is_transitive_reduction=False,
            universal_dep=None,
            library_crates=set(),
        )

    # Detect transitive reduction layout
    caption_region = tex_content[
        graph_match.start():graph_match.end() + 500
    ]
    is_transitive_reduction = bool(
        re.search(r"transitive\s+reduction", caption_region, re.IGNORECASE)
    )

    # Build node-name-to-crate-name mapping from \node declarations.
    # Labels are like: \textbf{neuronprompter-core}\\Domain types...
    # or: \textbf{neuronprompter}\\Binary entry...
    node_map: dict[str, str] = {}
    # Match \node[...] (id) at (x,y) { ... \textbf{crate-name} ... }
    # The at (x,y) part is optional
    node_pattern = (
        r"\\node\[.*?\]\s*\((\w+)\)\s*(?:at\s*\([^)]+\)\s*)?"
        r"\{((?:[^{}]|\{[^{}]*\})*)\}"
    )
    for match in re.finditer(node_pattern, graph_body):
        node_id = match.group(1)
        node_content = match.group(2)
        # Extract crate name from \textbf{...}
        name_match = re.search(r"\\textbf\{([^}]+)\}", node_content)
        if name_match:
            raw_label = name_match.group(1).strip()
            crate_name = sanitize_latex(raw_label)
            node_map[node_id] = crate_name

    # Identify library crate nodes (all except the binary entry point)
    library_crates = {
        name for name in node_map.values()
        if name != "neuronprompter"
    }

    # Detect "All library crates depend on <crate>" annotation
    universal_dep: str | None = None
    universal_match = re.search(
        r"All\s+library\s+crates\b.*?depend\s+.*?on"
        r".*?\\texttt\{([^}]+)\}",
        graph_body,
        re.DOTALL,
    )
    if universal_match:
        universal_dep = sanitize_latex(universal_match.group(1).strip())

    # Parse \draw[dep] edges.  Supported connector syntaxes:
    #   \draw[dep] (source) -- (target);
    #   \draw[dep] (source) to[...] (target);
    #   \draw[dep] (source.east) -| (target.north);
    #   \draw[dep] (source.east) |- (target.north);
    edge_pattern = (
        r"\\draw\[(?:bin)?dep[^\]]*\]\s*"
        r"\((\w+)(?:\.[^)]+)?\)\s*"
        r"(?:--|-\||\|-|\s*to\[.*?\])\s*"
        r"\((\w+)(?:\.[^)]+)?\)"
    )
    for match in re.finditer(edge_pattern, graph_body):
        source_id = match.group(1)
        target_id = match.group(2)
        source_crate = node_map.get(source_id)
        target_crate = node_map.get(target_id)
        if source_crate and target_crate:
            if source_crate not in deps:
                deps[source_crate] = set()
            deps[source_crate].add(target_crate)

    return DependencyGraphInfo(
        direct_edges=deps,
        is_transitive_reduction=is_transitive_reduction,
        universal_dep=universal_dep,
        library_crates=library_crates,
    )


# ---------------------------------------------------------------------------
# Rust source tree scanning routines
# ---------------------------------------------------------------------------

def parse_workspace_members(cargo_toml_path: Path) -> list[str]:
    """Parse the [workspace.members] list from the root Cargo.toml.

    Uses a simple line-by-line parser since we only need the workspace
    members array and do not require a full TOML parser.

    Args:
        cargo_toml_path: Absolute path to the root Cargo.toml file.

    Returns:
        A list of member directory paths relative to the workspace root
        (e.g., ["crates/neuronprompter-core", "crates/neuronprompter-db"]).
    """
    members: list[str] = []
    content = cargo_toml_path.read_text(encoding="utf-8")

    members_match = re.search(
        r"members\s*=\s*\[(.*?)\]",
        content,
        re.DOTALL,
    )
    if members_match is None:
        return members

    members_block = members_match.group(1)
    for string_match in re.finditer(r'"([^"]+)"', members_block):
        members.append(string_match.group(1))

    return members


def extract_crate_name_from_member(member_path: str) -> str:
    """Derive the crate name from a workspace member path.

    Workspace members are paths like "crates/neuronprompter-core".  The
    crate name is the last path component.

    Args:
        member_path: The member path from Cargo.toml.

    Returns:
        The crate name string.
    """
    return member_path.rstrip("/").split("/")[-1]


def collect_source_files(crate_dir: Path) -> set[str]:
    """Recursively collect all .rs files under a crate's src/ directory.

    Returns paths relative to the crate directory (e.g., "src/lib.rs",
    "src/domain/user.rs").  Files in tests/ and benches/ directories are
    excluded.

    Args:
        crate_dir: Absolute path to the crate directory.

    Returns:
        A set of relative path strings for all .rs source files.
    """
    src_dir = crate_dir / "src"
    if not src_dir.is_dir():
        return set()

    files: set[str] = set()
    for rs_file in src_dir.rglob("*.rs"):
        rel = rs_file.relative_to(crate_dir)
        rel_str = str(rel).replace("\\", "/")
        files.add(rel_str)

    return files


def _classify_rust_visibility(vis_prefix: Optional[str]) -> str:
    """Classify a Rust visibility modifier into "pub" or "priv".

    Only an unqualified ``pub`` without parenthesized restriction counts as
    "pub".  All restricted forms (``pub(crate)``, ``pub(super)``, etc.) and
    plain ``mod`` count as "priv".

    Args:
        vis_prefix: The visibility modifier string captured by the regex,
            or None if the module has no visibility qualifier.

    Returns:
        "pub" if the module is fully public, "priv" otherwise.
    """
    if vis_prefix is None:
        return "priv"

    trimmed = vis_prefix.strip()
    if trimmed == "pub":
        return "pub"

    return "priv"


def parse_mod_declarations(
    entry_file_path: Path,
) -> tuple[dict[str, str], dict[str, str]]:
    """Parse mod declarations from a lib.rs or main.rs file.

    Identifies module declarations and their visibility (``pub mod``,
    ``pub(crate) mod``, or plain ``mod``), and detects
    ``#[cfg(feature = "...")]`` attributes preceding mod statements.

    Args:
        entry_file_path: Absolute path to the lib.rs or main.rs file.

    Returns:
        A tuple of two dicts:
        - mod_visibility: maps module name to "pub" or "priv"
        - feature_gates: maps module name to the feature flag string
    """
    mod_visibility: dict[str, str] = {}
    feature_gates: dict[str, str] = {}

    if not entry_file_path.is_file():
        return mod_visibility, feature_gates

    content = entry_file_path.read_text(encoding="utf-8")
    lines = content.split("\n")

    pending_feature: Optional[str] = None

    for line in lines:
        stripped = line.strip()

        # Detect cfg(feature) attributes
        cfg_match = re.match(
            r'#\[cfg\(feature\s*=\s*"([^"]+)"\)\]',
            stripped,
        )
        if cfg_match:
            pending_feature = cfg_match.group(1)
            continue

        # Detect module declarations with any visibility modifier
        mod_match = re.match(
            r"(pub\s*(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;",
            stripped,
        )
        if mod_match:
            vis_prefix = mod_match.group(1)
            mod_name = mod_match.group(2)
            mod_visibility[mod_name] = _classify_rust_visibility(vis_prefix)
            if pending_feature:
                feature_gates[mod_name] = pending_feature
            pending_feature = None
            continue

        # Reset pending feature if the line is not a mod declaration
        # and not a blank, comment, or attribute line
        if stripped and not stripped.startswith("//") and not stripped.startswith("#["):
            pending_feature = None

    return mod_visibility, feature_gates


def parse_crate_cargo_toml(
    cargo_toml_path: Path,
    workspace_crate_names: set[str],
) -> tuple[set[str], set[str]]:
    """Parse a crate-level Cargo.toml for features and internal dependencies.

    Extracts feature flag names from the [features] section and identifies
    internal workspace dependencies from the [dependencies] section by
    checking against the known workspace crate names.

    Args:
        cargo_toml_path: Absolute path to the crate's Cargo.toml.
        workspace_crate_names: Set of all crate names in the workspace.

    Returns:
        A tuple of:
        - features: set of feature flag names defined in [features]
        - internal_deps: set of workspace crate names this crate depends on
    """
    features: set[str] = set()
    internal_deps: set[str] = set()

    if not cargo_toml_path.is_file():
        return features, internal_deps

    content = cargo_toml_path.read_text(encoding="utf-8")

    # Parse [features] section
    features_match = re.search(
        r"\[features\]\s*\n((?:.*\n)*?)\n*(?:\[|\Z)", content,
    )
    if features_match:
        features_block = features_match.group(1)
        for feat_match in re.finditer(r"^(\S+)\s*=", features_block, re.MULTILINE):
            feat_name = feat_match.group(1).strip()
            if feat_name != "default":
                features.add(feat_name)

    # Parse [dependencies] section for internal workspace crate deps
    deps_match = re.search(
        r"\[dependencies\]\s*\n((?:.*\n)*?)\n*(?:\[|\Z)", content,
    )
    if deps_match:
        deps_block = deps_match.group(1)
        for dep_match in re.finditer(r"^([\w-]+)\s*=", deps_block, re.MULTILINE):
            dep_name = dep_match.group(1).strip()
            normalized = dep_name.replace("_", "-")
            if normalized in workspace_crate_names:
                internal_deps.add(normalized)

    # Also check [dependencies.crate-name] style entries
    inline_dep_pattern = r"\[dependencies\.([\w-]+)\]"
    for dep_match in re.finditer(inline_dep_pattern, content):
        dep_name = dep_match.group(1).strip()
        normalized = dep_name.replace("_", "-")
        if normalized in workspace_crate_names:
            internal_deps.add(normalized)

    return features, internal_deps


def scan_rust_workspace(root: Path) -> tuple[list[str], dict[str, RustCrateInfo]]:
    """Scan the entire Rust workspace and collect information about each crate.

    Parses the root Cargo.toml for workspace members, then iterates over
    each crate directory to collect source files, mod declarations, feature
    gates, feature definitions, and internal dependencies.

    Args:
        root: Absolute path to the workspace root directory.

    Returns:
        A tuple of:
        - member_crate_names: list of crate names from workspace members
        - crate_infos: dict mapping crate name to RustCrateInfo
    """
    cargo_toml_path = root / "Cargo.toml"
    members = parse_workspace_members(cargo_toml_path)

    member_crate_names = [extract_crate_name_from_member(m) for m in members]
    workspace_crate_names = set(member_crate_names)

    crate_infos: dict[str, RustCrateInfo] = {}

    for member_path in members:
        crate_dir = root / member_path.replace("/", os.sep)
        crate_name = extract_crate_name_from_member(member_path)

        info = RustCrateInfo(name=crate_name, path=crate_dir)

        # Collect .rs source files
        info.source_files = collect_source_files(crate_dir)

        # Parse mod declarations from lib.rs or main.rs
        lib_rs = crate_dir / "src" / "lib.rs"
        main_rs = crate_dir / "src" / "main.rs"
        entry_file = lib_rs if lib_rs.is_file() else main_rs

        mod_vis, feat_gates = parse_mod_declarations(entry_file)
        info.mod_declarations = mod_vis
        info.feature_gates = feat_gates

        # Also parse mod declarations from subdirectory mod.rs files for
        # deeper module hierarchies (e.g., src/domain/mod.rs, src/repo/mod.rs)
        for src_file in sorted(info.source_files):
            if src_file.endswith("/mod.rs") and src_file != "src/mod.rs":
                sub_mod_path = crate_dir / src_file.replace("/", os.sep)
                sub_vis, sub_gates = parse_mod_declarations(sub_mod_path)
                # Prefix sub-module names with their parent directory
                parent_dir = src_file.replace("/mod.rs", "").replace("src/", "")
                for mod_name, visibility in sub_vis.items():
                    prefixed = f"{parent_dir}/{mod_name}"
                    info.mod_declarations[prefixed] = visibility
                for mod_name, gate in sub_gates.items():
                    prefixed = f"{parent_dir}/{mod_name}"
                    info.feature_gates[prefixed] = gate

        # Parse crate Cargo.toml for features and internal dependencies
        crate_cargo = crate_dir / "Cargo.toml"
        info.features, info.internal_deps = parse_crate_cargo_toml(
            crate_cargo, workspace_crate_names,
        )

        crate_infos[crate_name] = info

    return member_crate_names, crate_infos


# ---------------------------------------------------------------------------
# Module path to mod-declaration name mapping utilities
# ---------------------------------------------------------------------------

def file_path_to_mod_name(file_path: str) -> Optional[str]:
    """Convert a source file path to its corresponding mod declaration name.

    Maps paths like ``src/types.rs`` -> ``types``,
    ``src/domain/mod.rs`` -> ``domain``,
    ``src/domain/user.rs`` -> ``domain/user``.
    Returns None for entry point files (lib.rs, main.rs).

    Args:
        file_path: Relative path string (e.g., "src/domain/user.rs").

    Returns:
        The mod declaration name, or None for lib.rs/main.rs.
    """
    if file_path.startswith("src/"):
        inner = file_path[4:]
    else:
        return None

    if inner in ("lib.rs", "main.rs"):
        return None

    if inner.endswith("/mod.rs"):
        return inner[:-len("/mod.rs")]

    if inner.endswith(".rs"):
        return inner[:-3]

    return None


def mod_name_to_top_level(mod_name: str) -> str:
    """Extract the top-level module name from a potentially nested mod path.

    For example, ``domain/user`` -> ``domain``, ``types`` -> ``types``.

    Args:
        mod_name: The module name, potentially containing slashes for nesting.

    Returns:
        The top-level (first component) module name.
    """
    return mod_name.split("/")[0]


# ---------------------------------------------------------------------------
# Validation check implementations
# ---------------------------------------------------------------------------

def check_workspace_members(
    latex_crates: list[CrateSpec],
    cargo_crate_names: list[str],
) -> list[Diagnostic]:
    """Check 1: workspace_members (error severity).

    Verifies that every crate in the LaTeX crate overview table exists in
    the Cargo.toml workspace members, and vice versa.

    Args:
        latex_crates: Crate specs parsed from the LaTeX document.
        cargo_crate_names: Crate names from workspace Cargo.toml.

    Returns:
        A list of diagnostics for mismatches.
    """
    diagnostics: list[Diagnostic] = []
    latex_names = {c.name for c in latex_crates}
    cargo_names = set(cargo_crate_names)

    for name in sorted(latex_names - cargo_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="workspace_members",
            crate_name=name,
            detail=(
                "crate listed in architecture document but absent from "
                "Cargo.toml [workspace.members]"
            ),
        ))

    for name in sorted(cargo_names - latex_names):
        diagnostics.append(Diagnostic(
            level="error",
            category="workspace_members",
            crate_name=name,
            detail=(
                "crate in Cargo.toml [workspace.members] but absent from "
                "architecture document crate overview table"
            ),
        ))

    return diagnostics


def check_missing_source_files(
    latex_modules: list[ModuleEntry],
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 2: missing_source_files (error severity).

    Verifies that every file listed in a module table exists on disk.

    Args:
        latex_modules: Module entries parsed from the LaTeX document.
        crate_infos: Scanned Rust crate information.

    Returns:
        A list of diagnostics for missing files.
    """
    diagnostics: list[Diagnostic] = []

    for entry in latex_modules:
        crate_info = crate_infos.get(entry.crate_name)
        if crate_info is None:
            continue

        if entry.file_path not in crate_info.source_files:
            diagnostics.append(Diagnostic(
                level="error",
                category="missing_source_files",
                crate_name=entry.crate_name,
                detail=(
                    f"{entry.file_path} listed in architecture document "
                    f"but not found on disk"
                ),
            ))

    return diagnostics


def _is_excluded_file(file_path: str) -> bool:
    """Determine whether a source file should be excluded from documentation checks.

    Files in tests/ and benches/ directories and build.rs are excluded.
    mod.rs files are also excluded since they are implicitly part of the
    module system (the directory module itself is what gets documented).

    Args:
        file_path: Relative path string (e.g., "src/domain/mod.rs").

    Returns:
        True if the file should be excluded from undocumented checks.
    """
    parts = file_path.replace("\\", "/").split("/")
    if "tests" in parts or "benches" in parts:
        return True
    if file_path.endswith("build.rs"):
        return True
    # mod.rs files are implicitly part of the module system; the directory
    # module itself is documented rather than the mod.rs file
    if file_path.endswith("/mod.rs"):
        return True
    return False


def _is_entry_point(file_path: str) -> bool:
    """Determine whether a source file is a crate entry point."""
    return file_path in ("src/lib.rs", "src/main.rs")


def check_undocumented_source_files(
    latex_modules: list[ModuleEntry],
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 3: undocumented_source_files (error severity).

    Verifies that every .rs file in a crate's src/ directory is documented
    in the LaTeX module structure table.  Entry points (lib.rs, main.rs)
    for crates without module tables, mod.rs files, and test/bench files
    are excluded.

    Args:
        latex_modules: Module entries parsed from the LaTeX document.
        crate_infos: Scanned Rust crate information.

    Returns:
        A list of diagnostics for undocumented files.
    """
    diagnostics: list[Diagnostic] = []

    documented: dict[str, set[str]] = {}
    for entry in latex_modules:
        if entry.crate_name not in documented:
            documented[entry.crate_name] = set()
        documented[entry.crate_name].add(entry.file_path)

    crates_with_module_table = set(documented.keys())

    for crate_name, info in sorted(crate_infos.items()):
        crate_documented = documented.get(crate_name, set())
        has_module_table = crate_name in crates_with_module_table

        for file_path in sorted(info.source_files):
            if _is_excluded_file(file_path):
                continue
            if not has_module_table and _is_entry_point(file_path):
                continue
            if file_path not in crate_documented:
                diagnostics.append(Diagnostic(
                    level="error",
                    category="undocumented_source_files",
                    crate_name=crate_name,
                    detail=(
                        f"{file_path} exists on disk but not documented "
                        f"in architecture document"
                    ),
                ))

    return diagnostics


def check_visibility_mismatch(
    latex_modules: list[ModuleEntry],
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 4: visibility_mismatch (warning severity).

    Verifies that the pub/private visibility declared in the LaTeX document
    matches the actual mod declaration in the crate's lib.rs or parent
    mod.rs.  Only checks modules where visibility is documented.

    Args:
        latex_modules: Module entries parsed from the LaTeX document.
        crate_infos: Scanned Rust crate information.

    Returns:
        A list of diagnostics for visibility mismatches.
    """
    diagnostics: list[Diagnostic] = []

    for entry in latex_modules:
        if entry.visibility is None:
            continue

        crate_info = crate_infos.get(entry.crate_name)
        if crate_info is None:
            continue

        mod_name = file_path_to_mod_name(entry.file_path)
        if mod_name is None:
            continue

        top_level = mod_name_to_top_level(mod_name)

        actual_vis = crate_info.mod_declarations.get(mod_name)
        if actual_vis is None:
            actual_vis = crate_info.mod_declarations.get(top_level)

        if actual_vis is None:
            continue

        expected_vis = "pub" if entry.visibility == "public" else "priv"

        if mod_name == top_level:
            if actual_vis != expected_vis:
                label_actual = (
                    "pub mod" if actual_vis == "pub" else "mod (private)"
                )
                diagnostics.append(Diagnostic(
                    level="warning",
                    category="visibility_mismatch",
                    crate_name=entry.crate_name,
                    detail=(
                        f"{entry.file_path} declared {entry.visibility} "
                        f"in docs but {label_actual} in source"
                    ),
                ))
        else:
            nested_vis = crate_info.mod_declarations.get(mod_name)
            if nested_vis is not None and nested_vis != expected_vis:
                label_actual = (
                    "pub mod" if nested_vis == "pub" else "mod (private)"
                )
                parent_mod_file = (
                    f"src/{'/'.join(mod_name.split('/')[:-1])}/mod.rs"
                )
                diagnostics.append(Diagnostic(
                    level="warning",
                    category="visibility_mismatch",
                    crate_name=entry.crate_name,
                    detail=(
                        f"{entry.file_path} declared {entry.visibility} "
                        f"in docs but {label_actual} in {parent_mod_file}"
                    ),
                ))

    return diagnostics


def check_feature_gates(
    latex_modules: list[ModuleEntry],
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 5: feature_gates (error severity).

    Verifies that feature-gated modules (``#[cfg(feature = "...")]``) in the
    source code are documented with the correct feature flag in the
    architecture document, and vice versa.

    Args:
        latex_modules: Module entries parsed from the LaTeX document.
        crate_infos: Scanned Rust crate information.

    Returns:
        A list of diagnostics for feature gate mismatches.
    """
    diagnostics: list[Diagnostic] = []

    for entry in latex_modules:
        if entry.feature_flag is None:
            continue

        crate_info = crate_infos.get(entry.crate_name)
        if crate_info is None:
            continue

        mod_name = file_path_to_mod_name(entry.file_path)
        if mod_name is None:
            continue

        top_level = mod_name_to_top_level(mod_name)
        actual_gate = crate_info.feature_gates.get(top_level)

        if actual_gate is None:
            diagnostics.append(Diagnostic(
                level="error",
                category="feature_gates",
                crate_name=entry.crate_name,
                detail=(
                    f"{entry.file_path} listed under feature "
                    f'"{entry.feature_flag}" in docs but no '
                    f"#[cfg(feature)] on mod declaration in source"
                ),
            ))
        elif actual_gate != entry.feature_flag:
            diagnostics.append(Diagnostic(
                level="error",
                category="feature_gates",
                crate_name=entry.crate_name,
                detail=(
                    f"{entry.file_path} listed under feature "
                    f'"{entry.feature_flag}" in docs but gated by '
                    f'"{actual_gate}" in source'
                ),
            ))

    return diagnostics


def check_undocumented_features(
    latex_features: set[str],
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 6: undocumented_features (error severity).

    Verifies that every feature flag defined in a crate's Cargo.toml
    [features] section is described in the architecture document.

    Args:
        latex_features: Feature flag names from the LaTeX document.
        crate_infos: Scanned Rust crate information.

    Returns:
        A list of diagnostics for undocumented features.
    """
    diagnostics: list[Diagnostic] = []

    for crate_name, info in sorted(crate_infos.items()):
        for feature in sorted(info.features):
            if feature not in latex_features:
                diagnostics.append(Diagnostic(
                    level="error",
                    category="undocumented_features",
                    crate_name=crate_name,
                    detail=(
                        f'feature "{feature}" defined in Cargo.toml but '
                        f"not documented in architecture document"
                    ),
                ))

    return diagnostics


def check_dependency_graph(
    graph_info: DependencyGraphInfo,
    crate_infos: dict[str, RustCrateInfo],
) -> list[Diagnostic]:
    """Check 7: dependency_graph (error severity).

    Compares the crate-to-crate dependency edges documented in the LaTeX
    TikZ dependency graph against the actual dependencies declared in each
    crate's Cargo.toml.

    Forward check ("in document but not in Cargo.toml"):
        Uses only the directly drawn edges.

    Reverse check ("in Cargo.toml but not in document"):
        When the graph is a transitive reduction, computes the transitive
        closure of the drawn edges.  Additionally, when a universal
        dependency annotation exists, that dependency is suppressed from
        the reverse check for all library crate nodes.

    Args:
        graph_info: Parsed dependency graph with direct edges and metadata.
        crate_infos: Parsed Cargo.toml metadata per workspace crate.

    Returns:
        A list of diagnostics for dependency graph deviations.
    """
    diagnostics: list[Diagnostic] = []
    direct = graph_info.direct_edges

    # For the reverse check, compute effective reachable deps
    if graph_info.is_transitive_reduction:
        reachable = _transitive_closure(direct)
    else:
        reachable = direct

    # When the graph declares a universal dependency, add it to the
    # reachable set for every crate node
    if graph_info.universal_dep:
        all_graph_crates = graph_info.library_crates | {
            name for name in direct
        }
        for crate_name in all_graph_crates:
            if crate_name == graph_info.universal_dep:
                continue
            if crate_name not in reachable:
                reachable[crate_name] = set()
            reachable[crate_name].add(graph_info.universal_dep)

    all_crate_names = set(crate_infos.keys())

    for crate_name in sorted(all_crate_names):
        info = crate_infos.get(crate_name)
        if info is None:
            continue

        actual_deps = info.internal_deps
        drawn_deps = direct.get(crate_name, set())
        reachable_deps = reachable.get(crate_name, set())

        # Forward check: drawn edge not in Cargo.toml
        for dep in sorted(drawn_deps - actual_deps):
            diagnostics.append(Diagnostic(
                level="error",
                category="dependency_graph",
                crate_name=crate_name,
                detail=(
                    f"depends on {dep} in architecture document "
                    f"but not in Cargo.toml [dependencies]"
                ),
            ))

        # Reverse check: Cargo.toml dep not reachable in graph
        for dep in sorted(actual_deps - reachable_deps):
            diagnostics.append(Diagnostic(
                level="error",
                category="dependency_graph",
                crate_name=crate_name,
                detail=(
                    f"depends on {dep} in Cargo.toml but not depicted "
                    f"in architecture document dependency graph"
                ),
            ))

    return diagnostics


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------

def format_human_readable(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as human-readable text lines.

    Each line has the format::

        [ERROR|WARN] <category>: <crate-name> -- <detail>

    A summary line at the end reports total error and warning counts.

    Args:
        diagnostics: List of diagnostic instances to format.

    Returns:
        A multi-line formatted string.
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
            f"{prefix} {diag.category}: {diag.crate_name} -- {diag.detail}"
        )

    lines.append(f"SUMMARY: {error_count} error(s), {warning_count} warning(s)")
    return "\n".join(lines)


def format_json(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as a JSON string.

    The JSON object has three keys: "errors", "warnings", and "summary".

    Args:
        diagnostics: List of diagnostic instances to format.

    Returns:
        A JSON-formatted string with indentation.
    """
    errors = []
    warnings = []

    for diag in diagnostics:
        entry = {
            "category": diag.category,
            "crate": diag.crate_name,
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

def run_validation(tex_path: Path, root_path: Path) -> list[Diagnostic]:
    """Execute all seven validation checks and return the combined diagnostics.

    Parses the LaTeX document for crate specs, module entries, feature flags,
    and dependency edges.  Scans the Rust workspace for actual source files,
    mod declarations, and Cargo.toml metadata.  Then runs each check.

    Args:
        tex_path: Absolute path to the LaTeX architecture document.
        root_path: Absolute path to the Cargo workspace root.

    Returns:
        A list of all diagnostics from all seven checks.
    """
    tex_content = tex_path.read_text(encoding="utf-8")

    latex_crates = parse_crate_overview(tex_content)
    latex_modules = parse_module_tables(tex_content)
    latex_features = parse_feature_flags(tex_content)
    graph_info = parse_dependency_graph(tex_content)

    cargo_crate_names, crate_infos = scan_rust_workspace(root_path)

    all_diagnostics: list[Diagnostic] = []

    # Check 1: workspace_members
    all_diagnostics.extend(
        check_workspace_members(latex_crates, cargo_crate_names)
    )
    # Check 2: missing_source_files
    all_diagnostics.extend(
        check_missing_source_files(latex_modules, crate_infos)
    )
    # Check 3: undocumented_source_files
    all_diagnostics.extend(
        check_undocumented_source_files(latex_modules, crate_infos)
    )
    # Check 4: visibility_mismatch
    all_diagnostics.extend(
        check_visibility_mismatch(latex_modules, crate_infos)
    )
    # Check 5: feature_gates
    all_diagnostics.extend(
        check_feature_gates(latex_modules, crate_infos)
    )
    # Check 6: undocumented_features
    all_diagnostics.extend(
        check_undocumented_features(latex_features, crate_infos)
    )
    # Check 7: dependency_graph
    all_diagnostics.extend(
        check_dependency_graph(graph_info, crate_infos)
    )

    return all_diagnostics


def main() -> int:
    """Parse command-line arguments, run validation, and produce output.

    Returns:
        Exit code: 0 if all checks pass, 1 if discrepancies found, 2 if
        the script cannot parse the LaTeX file or locate the workspace root.
    """
    parser = argparse.ArgumentParser(
        description=(
            "Architecture-Code Consistency Validator for NeuronPrompter. "
            "Enforces bidirectional consistency between the LaTeX architecture "
            "document and the Rust source tree."
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
        "--root",
        required=True,
        type=Path,
        help="Path to the Cargo workspace root directory",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help=(
            "Treat warnings as errors (used in CI to block the pipeline "
            "on any discrepancy)"
        ),
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON instead of human-readable text",
    )
    args = parser.parse_args()

    tex_path: Path = args.tex.resolve()
    root_path: Path = args.root.resolve()

    if not tex_path.is_file():
        print(
            f"Error: LaTeX file not found: {tex_path}",
            file=sys.stderr,
        )
        return 2

    if not root_path.is_dir():
        print(
            f"Error: workspace root directory not found: {root_path}",
            file=sys.stderr,
        )
        return 2

    cargo_toml = root_path / "Cargo.toml"
    if not cargo_toml.is_file():
        print(
            f"Error: Cargo.toml not found in workspace root: {cargo_toml}",
            file=sys.stderr,
        )
        return 2

    try:
        diagnostics = run_validation(tex_path, root_path)
    except Exception as exc:
        print(
            f"Error: failed to run validation: {exc}",
            file=sys.stderr,
        )
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
