#!/usr/bin/env python3
"""Test Catalog Validator for NeuronPrompter.

Validates bidirectional consistency between the LaTeX architecture document's
test catalogs and the actual Rust test code.  Detects missing tests, count
mismatches, numbering inconsistencies, and arithmetic errors in the summary
table.

Validation checks (8 categories):

  1.  test_id_duplicates       (error)   No duplicate test IDs in LaTeX or code
  2.  test_id_sequential       (warning) IDs within each prefix are sequential
  3.  catalog_count            (error)   Catalog entry count matches caption claim
  4.  summary_arithmetic       (error)   Summary subtotals and total are correct
  5.  summary_vs_catalog       (error)   Summary per-crate counts match catalog
  6.  latex_tests_in_code      (error)   Every LaTeX test ID exists in code
  7.  code_tests_in_latex      (error)   Every code test ID exists in LaTeX
  8.  fn_name_doc_id           (error)   Function name ID matches doc-comment ID

Exit codes:
  0  All checks passed (no errors; warnings allowed unless --strict).
  1  At least one error found, or a warning with --strict enabled.
  2  Script cannot run (missing files, parse failure).

Workspace crates:
  neuronprompter-core, neuronprompter-db, neuronprompter-application,
  neuronprompter-api, neuronprompter-mcp, neuronprompter-web,
  neuronprompter (binary)

Gracefully handles the bootstrap case where no test catalog exists in the
LaTeX document and no test IDs exist in code.  When both sides are empty the
validator passes cleanly; when only one side is populated the appropriate
cross-reference errors are raised.

Usage:
  python tools/ci/validate_tests.py \\
      --tex docs/architecture/architecture.tex \\
      --root . [--strict] [--json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


# ---------------------------------------------------------------------------
# LaTeX \input{} resolution
# ---------------------------------------------------------------------------

def resolve_inputs(tex_content: str, tex_dir: Path) -> str:
    r"""Resolve ``\input{filename}`` directives by inlining referenced files.

    Searches for ``\input{name}`` commands in the LaTeX content and replaces
    each with the full contents of the referenced file.  The file path is
    resolved relative to *tex_dir* (the directory containing the main ``.tex``
    file).  The ``.tex`` extension is appended if the filename does not
    already end with it, following the standard LaTeX ``\input`` convention.

    Unresolvable ``\input`` commands (file not found on disk) are left in
    place without modification.

    Args:
        tex_content: The raw LaTeX document content.
        tex_dir: The directory containing the main ``.tex`` file.

    Returns:
        The LaTeX content with all resolvable ``\input{}`` directives
        replaced by the contents of the referenced files.
    """
    def _replace_input(match: re.Match) -> str:
        filename = match.group(1)
        if not filename.endswith(".tex"):
            filename += ".tex"
        input_path = tex_dir / filename
        if input_path.is_file():
            return input_path.read_text(encoding="utf-8")
        return match.group(0)

    return re.sub(r"\\input\{([^}]+)\}", _replace_input, tex_content)


# ---------------------------------------------------------------------------
# Word-to-number mapping for prose and caption claims.
# Covers the range relevant to NeuronPrompter's test counts (1-30).
# ---------------------------------------------------------------------------
WORD_TO_NUMBER: dict[str, int] = {
    "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
    "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
    "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14,
    "fifteen": 15, "sixteen": 16, "seventeen": 17, "eighteen": 18,
    "nineteen": 19, "twenty": 20, "twenty-one": 21, "twenty-two": 22,
    "twenty-three": 23, "twenty-four": 24, "twenty-five": 25,
    "twenty-six": 26, "twenty-seven": 27, "twenty-eight": 28,
    "twenty-nine": 29, "thirty": 30,
}


# ---------------------------------------------------------------------------
# Test ID regex pattern
# ---------------------------------------------------------------------------
# Matches test IDs like:
#   T-CORE-001, T-DB-001, T-APP-001a, T-MCP-AUTH-001, T-WEB-003
# Structure: T-PREFIX[-SUBPREFIX]-NNN[letter]
# Prefix segments start with an uppercase letter and may contain digits
# (e.g., E2E).  The numeric part is always exactly 3 digits.
TEST_ID_PATTERN = re.compile(
    r"T-[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{3}[a-z]?"
)


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------

@dataclass
class Diagnostic:
    """A single validation diagnostic (error, warning, or info).

    Attributes:
        level: Either ``"error"``, ``"warning"``, or ``"info"``.
        category: The check category identifier (e.g.,
            ``"test_id_duplicates"``, ``"catalog_count"``).
        section: The section or entity group this diagnostic applies to
            (e.g., ``"unit_tests"``, ``"summary_table"``).
        detail: A human-readable description of the discrepancy.
    """

    level: str
    category: str
    section: str
    detail: str


# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------

def _join_multiline_rows(table_body: str) -> list[str]:
    r"""Join LaTeX table rows that span multiple source lines.

    LaTeX table rows end with ``\\`` (double backslash).  Source lines that
    do not end with ``\\`` are continuations of the current row.  This
    function concatenates continuation lines and returns a list of
    complete logical rows.

    Skips structural commands (``\toprule``, ``\midrule``, ``\bottomrule``,
    ``\endfirsthead``, ``\endhead``, ``\endfoot``, ``\endlastfoot``) and
    empty lines.

    Args:
        table_body: The text between ``\begin{longtable/tabularx}`` and
            ``\end{longtable/tabularx}``, with the column specification
            already removed.

    Returns:
        A list of complete row strings, each ending with ``\\``.
    """
    structural = {
        "\\toprule", "\\midrule", "\\bottomrule",
        "\\endfirsthead", "\\endhead", "\\endfoot", "\\endlastfoot",
    }

    rows: list[str] = []
    current_parts: list[str] = []

    for line in table_body.splitlines():
        stripped = line.strip()

        # Skip empty lines and structural commands
        if not stripped:
            continue
        if stripped in structural:
            continue
        # Skip caption lines
        if stripped.startswith("\\caption"):
            continue

        current_parts.append(stripped)

        # A row is complete when it ends with \\ (possibly followed by
        # whitespace or a comment)
        if re.search(r"\\\\(\s*(%.*)?)?$", stripped):
            rows.append(" ".join(current_parts))
            current_parts = []

    # If there are leftover parts (row without trailing \\), include them
    if current_parts:
        rows.append(" ".join(current_parts))

    return rows


def _find_table_body(tex_content: str, caption_pattern: str) -> str | None:
    r"""Locate a LaTeX table/longtable environment by its caption text.

    Searches for the caption matching the given pattern, then finds the
    enclosing environment (``tabularx`` or ``longtable``).  Returns the text
    between the environment begin and end markers.

    Args:
        tex_content: The full LaTeX document content.
        caption_pattern: A regex pattern to match in the ``\caption{...}``
            text.

    Returns:
        The table body text, or ``None`` if the caption is not found.
    """
    caption_match = re.search(
        r"\\caption\{" + caption_pattern + r"[^}]*\}",
        tex_content,
        re.DOTALL,
    )
    if not caption_match:
        return None

    caption_pos = caption_match.start()

    # Search backward from caption for the nearest \begin{tabularx} or
    # \begin{longtable}
    begin_pattern = r"\\begin\{(tabularx|longtable)\}"
    begin_match = None
    for m in re.finditer(begin_pattern, tex_content):
        if m.start() < caption_pos:
            begin_match = m
        else:
            break

    if not begin_match:
        return None

    env_name = begin_match.group(1)
    end_pattern = r"\\end\{" + env_name + r"\}"
    end_match = re.search(end_pattern, tex_content[begin_match.end():])
    if not end_match:
        return None

    body_start = begin_match.end()
    body_end = begin_match.end() + end_match.start()
    return tex_content[body_start:body_end]


def _extract_caption_number(
    tex_content: str,
    caption_pattern: str,
) -> int | None:
    r"""Extract the numeric count from a test catalog caption.

    Captions follow the form: ``"Unit test catalog: 42 tests across..."``
    or ``"twenty-three tests covering..."``.  This function finds the caption
    matching the given pattern and extracts the first integer or word-form
    number found within it.

    Args:
        tex_content: The full LaTeX document content.
        caption_pattern: A regex pattern to match within the caption text.

    Returns:
        The integer count from the caption, or ``None`` if not found.
    """
    caption_match = re.search(
        r"\\caption\{(" + caption_pattern + r"[^}]*)\}",
        tex_content,
        re.DOTALL,
    )
    if not caption_match:
        return None

    caption_text = caption_match.group(1)

    # Try digit form first: "42 tests"
    num_match = re.search(r"(\d+)\s+(?:tests?|benchmarks?)", caption_text)
    if num_match:
        return int(num_match.group(1))

    # Try word form: "twenty-three tests"
    caption_lower = caption_text.lower()
    for word, value in sorted(
        WORD_TO_NUMBER.items(), key=lambda kv: -len(kv[0])
    ):
        pattern = rf"\b{re.escape(word)}\b\s+(?:tests?|benchmarks?)"
        if re.search(pattern, caption_lower):
            return value

    return None


def _extract_test_ids_from_body(body: str) -> list[str]:
    r"""Extract all test IDs from a LaTeX table body.

    Scans the joined rows of a longtable for test ID patterns in the
    first column.  The first column uses ``ttfamily`` formatting, so IDs
    appear as plain text (e.g., ``T-CORE-001``) without ``\texttt{}``
    wrapping.

    Skips ``\multicolumn`` rows (section headers) and ``\textbf`` rows
    (table headers).

    Args:
        body: The text content of a longtable environment.

    Returns:
        A list of test ID strings in document order.
    """
    test_ids: list[str] = []

    rows = _join_multiline_rows(body)
    for row in rows:
        # Skip header rows and section divider rows
        if "\\textbf" in row or "\\normalfont" in row:
            continue
        if "\\multicolumn" in row:
            continue

        # Extract test ID from the first cell
        cells = row.split("&")
        if not cells:
            continue

        first_cell = cells[0].strip().rstrip("\\").strip()
        match = TEST_ID_PATTERN.match(first_cell)
        if match:
            test_ids.append(match.group(0))

    return test_ids


def _base_prefix(test_id: str) -> str:
    """Extract the base prefix from a test ID for grouping purposes.

    Splits the ID after ``T-`` and before the numeric part.  For IDs with
    sub-prefixes (like ``T-MCP-AUTH-001``), the full prefix chain is
    returned.

    Examples::

        T-CORE-001     -> "CORE"
        T-MCP-AUTH-006 -> "MCP-AUTH"
        T-CORE-024a    -> "CORE"
        T-DB-014       -> "DB"

    Args:
        test_id: A test ID string (e.g., ``"T-CORE-001"``).

    Returns:
        The prefix portion without the ``T-`` leader and numeric suffix.
    """
    # Remove the T- prefix
    rest = test_id[2:]
    # Remove the numeric suffix (and optional letter suffix)
    match = re.search(r"-(\d{3}[a-z]?)$", rest)
    if match:
        return rest[:match.start()]
    return rest


def _numeric_part(test_id: str) -> int:
    """Extract the numeric portion of a test ID.

    For ``T-CORE-001`` returns ``1``, for ``T-MCP-AUTH-006`` returns ``6``.
    Ignores letter suffixes (``T-CORE-024a`` returns ``24``).

    Args:
        test_id: A test ID string.

    Returns:
        The integer value of the 3-digit numeric part.
    """
    match = re.search(r"(\d{3})[a-z]?$", test_id)
    if match:
        return int(match.group(1))
    return 0


# ---------------------------------------------------------------------------
# LaTeX parsing functions
# ---------------------------------------------------------------------------

def parse_test_catalogs(
    tex_content: str,
) -> list[tuple[str, list[str], int | None]]:
    """Parse all test catalog tables from the LaTeX document.

    Searches for any table whose caption contains ``"test catalog"``
    (case-insensitive).  For each match, extracts the test IDs from the
    table body and the claimed count from the caption.

    This is designed to be generic: it will pick up catalogs named
    ``"Unit test catalog"``, ``"Integration test catalog"``, etc.,
    without hard-coding each one.

    Args:
        tex_content: The full LaTeX document content (with ``\\input``
            resolved).

    Returns:
        A list of ``(catalog_name, test_ids, caption_count)`` tuples
        where *catalog_name* is the full caption text (cleaned),
        *test_ids* is the list of IDs parsed from the table body, and
        *caption_count* is the integer from the caption (or ``None``
        if not parseable).
    """
    catalogs: list[tuple[str, list[str], int | None]] = []

    # Find all captions containing "test catalog" (case-insensitive)
    for cap_match in re.finditer(
        r"\\caption\{([^}]*[Tt]est\s+[Cc]atalog[^}]*)\}",
        tex_content,
    ):
        caption_text = cap_match.group(1).strip()
        # Derive a safe section label from the caption
        label = re.sub(r"[^a-zA-Z0-9]+", "_", caption_text).strip("_").lower()

        # Use the literal caption text (escaped) as the search pattern
        escaped = re.escape(caption_text)
        body = _find_table_body(tex_content, escaped)
        if body is None:
            continue

        test_ids = _extract_test_ids_from_body(body)
        caption_count = _extract_caption_number(tex_content, escaped)
        catalogs.append((label, test_ids, caption_count))

    return catalogs


def parse_crate_sections(
    tex_content: str,
) -> dict[str, list[str]]:
    r"""Parse test catalog tables, grouping test IDs by crate section.

    The test catalog longtables may contain ``\multicolumn`` divider rows
    that identify the crate for the following tests.  The pattern is::

        \multicolumn{3}{l}{\textit{Crate: \texttt{neuronprompter-xxx} ...}}

    Args:
        tex_content: The full LaTeX document content.

    Returns:
        A dict mapping crate names (e.g., ``"neuronprompter-core"``) to
        their list of test IDs.  The special key ``"_unknown"`` collects
        any IDs that appear before the first crate header.
    """
    crate_sections: dict[str, list[str]] = {}

    # Search across all test catalog tables
    for cap_match in re.finditer(
        r"\\caption\{[^}]*[Tt]est\s+[Cc]atalog[^}]*\}",
        tex_content,
    ):
        caption_text = cap_match.group(0)
        # Extract the caption content for table lookup
        inner = re.search(r"\\caption\{([^}]+)\}", caption_text)
        if not inner:
            continue
        escaped = re.escape(inner.group(1).strip())
        body = _find_table_body(tex_content, escaped)
        if not body:
            continue

        current_crate = "_unknown"
        rows = _join_multiline_rows(body)
        for row in rows:
            # Check for crate section header
            if "\\multicolumn" in row and "\\texttt{neuronprompter" in row:
                crate_match = re.search(
                    r"\\texttt\{(neuronprompter[^}]*)\}", row,
                )
                if crate_match:
                    crate_name = crate_match.group(1).replace("\\_", "_")
                    crate_name = crate_name.replace("\\allowbreak", "")
                    current_crate = crate_name.strip()
                    if current_crate not in crate_sections:
                        crate_sections[current_crate] = []
                continue

            # Skip structural rows
            if "\\textbf" in row or "\\normalfont" in row:
                continue
            if "\\multicolumn" in row:
                continue

            # Extract test ID
            cells = row.split("&")
            if not cells:
                continue

            first_cell = cells[0].strip().rstrip("\\").strip()
            match = TEST_ID_PATTERN.match(first_cell)
            if match:
                if current_crate not in crate_sections:
                    crate_sections[current_crate] = []
                crate_sections[current_crate].append(match.group(0))

    return crate_sections


def parse_summary_table(tex_content: str) -> dict | None:
    r"""Parse the test summary table.

    The summary table has a caption containing ``"test summary"`` or
    ``"Test count"`` (case-insensitive) and lists category names, counts,
    and (optionally) test types.  Subtotal rows are italicized, and the
    total row is bolded.

    Args:
        tex_content: The full LaTeX document content.

    Returns:
        A dict with keys:

        - ``"rows"``: list of ``(category_name, count, test_type)`` tuples
        - ``"subtotals"``: dict mapping subtotal names to counts
        - ``"total"``: the total count from the table body
        - ``"caption_total"``: the count claimed in the caption

        Returns ``None`` if the table is not found.
    """
    # Try several possible caption patterns
    body: str | None = None
    caption_key: str | None = None

    for pattern in (
        r"[Tt]est\s+catalog\s+summary",
        r"[Tt]est\s+summary",
        r"[Tt]est\s+count",
    ):
        body = _find_table_body(tex_content, pattern)
        if body is not None:
            caption_key = pattern
            break

    if body is None or caption_key is None:
        return None

    rows_data: list[tuple[str, int, str]] = []
    subtotals: dict[str, int] = {}
    total: int | None = None

    rows = _join_multiline_rows(body)
    for row in rows:
        # Skip header rows
        if "\\textbf{Category}" in row or "\\textbf{Crate}" in row:
            continue

        cells = row.split("&")
        if len(cells) < 2:
            continue

        name_cell = cells[0].strip().rstrip("\\").strip()
        count_cell = cells[1].strip().rstrip("\\").strip()

        # Extract the numeric count (may be wrapped in \textit{} or
        # \textbf{})
        count_str = re.sub(r"\\textit\{([^}]*)\}", r"\1", count_cell)
        count_str = re.sub(r"\\textbf\{([^}]*)\}", r"\1", count_str)
        count_str = count_str.strip()

        if not count_str:
            continue

        try:
            count = int(count_str)
        except ValueError:
            continue

        # Clean the name cell of LaTeX formatting
        clean_name = re.sub(r"\\textit\{([^}]*)\}", r"\1", name_cell)
        clean_name = re.sub(r"\\textbf\{([^}]*)\}", r"\1", clean_name)
        clean_name = re.sub(r"\\texttt\{([^}]*)\}", r"\1", clean_name)
        clean_name = clean_name.strip()

        # Determine if this is a subtotal, total, or regular row
        if "\\textbf" in name_cell and "Total" in clean_name:
            total = count
        elif "\\textit" in name_cell and "subtotal" in clean_name.lower():
            subtotals[clean_name.lower()] = count
        else:
            # Regular data row: extract test type from third cell if present
            test_type = ""
            if len(cells) >= 3:
                test_type = cells[2].strip().rstrip("\\").strip()

            rows_data.append((clean_name, count, test_type))

    # Extract caption total
    caption_total = _extract_caption_number(tex_content, caption_key)

    return {
        "rows": rows_data,
        "subtotals": subtotals,
        "total": total,
        "caption_total": caption_total,
    }


# ---------------------------------------------------------------------------
# Code parsing functions
# ---------------------------------------------------------------------------

def parse_code_test_ids(root: Path) -> dict[str, list[str]]:
    """Extract all test IDs from Rust test code across the workspace.

    Scans all ``.rs`` files under ``crates/`` for doc comments containing
    test ID patterns (``/// T-XXX-NNN:``).  Groups the found IDs by their
    base prefix (e.g., ``"CORE"``, ``"DB"``, ``"MCP-AUTH"``).

    Args:
        root: The Cargo workspace root directory.

    Returns:
        A dict mapping base prefixes to their list of test IDs, e.g.,
        ``{"CORE": ["T-CORE-001", ...], "DB": ["T-DB-001", ...], ...}``.
    """
    crates_dir = root / "crates"
    ids_by_prefix: dict[str, list[str]] = {}

    if not crates_dir.is_dir():
        return ids_by_prefix

    for rs_file in crates_dir.rglob("*.rs"):
        try:
            content = rs_file.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue

        # Find all test ID doc comments: /// T-XXX-NNN:
        for match in re.finditer(
            r"///\s+(T-[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{3}[a-z]?):",
            content,
        ):
            test_id = match.group(1)
            prefix = _base_prefix(test_id)
            if prefix not in ids_by_prefix:
                ids_by_prefix[prefix] = []
            ids_by_prefix[prefix].append(test_id)

    return ids_by_prefix


def map_code_test_files(root: Path) -> dict[str, list[str]]:
    """Map each test ID to the source file(s) where it appears in code.

    Scans all ``.rs`` files for test ID doc comments and records the
    relative file path for each ID.  Used to provide helpful file
    locations in diagnostic messages.

    Args:
        root: The Cargo workspace root directory.

    Returns:
        A dict mapping test ID strings to lists of relative file paths
        where that ID appears.  Multiple files indicate a duplicate.
    """
    crates_dir = root / "crates"
    id_to_files: dict[str, list[str]] = {}

    if not crates_dir.is_dir():
        return id_to_files

    for rs_file in crates_dir.rglob("*.rs"):
        try:
            content = rs_file.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue

        rel_path = str(rs_file.relative_to(root)).replace("\\", "/")

        for match in re.finditer(
            r"///\s+(T-[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{3}[a-z]?):",
            content,
        ):
            test_id = match.group(1)
            if test_id not in id_to_files:
                id_to_files[test_id] = []
            id_to_files[test_id].append(rel_path)

    return id_to_files


def _doc_id_to_prefix_number(doc_id: str) -> tuple[str, str]:
    """Extract the lowercase prefix and numeric part from a doc-comment test ID.

    ``T-CORE-061``      returns ``("core", "061")``.
    ``T-MCP-AUTH-001``  returns ``("mcp_auth", "001")``.
    ``T-DB-007``        returns ``("db", "007")``.
    """
    rest = doc_id[2:]  # strip leading "T-"
    m = re.match(r"(.+)-(\d{3}[a-z]?)$", rest)
    if not m:
        return "", ""
    return m.group(1).lower().replace("-", "_"), m.group(2)


def _fn_name_to_prefix_number(fn_name: str) -> tuple[str, str] | None:
    """Extract prefix and numeric part from a test function name.

    ``t_core_061_desc``      returns ``("core", "061")``.
    ``t_mcp_auth_001_desc``  returns ``("mcp_auth", "001")``.
    ``t_db_007_desc``        returns ``("db", "007")``.

    Returns ``None`` if the function name does not follow the expected
    pattern.
    """
    parts = fn_name.split("_")
    if not parts or parts[0] != "t":
        return None
    for i in range(1, len(parts)):
        if re.fullmatch(r"\d{3}[a-z]?", parts[i]):
            prefix = "_".join(parts[1:i])
            number = parts[i]
            return prefix, number
    return None


# Regex matching a test function definition line (optional async keyword)
_FN_DEF_RE = re.compile(
    r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(t_\w+)\s*\("
)


# ---------------------------------------------------------------------------
# Validation check functions
# ---------------------------------------------------------------------------

def check_test_id_duplicates(
    latex_ids: list[str],
    code_id_files: dict[str, list[str]],
) -> list[Diagnostic]:
    """Check 1: Detect duplicate test IDs within LaTeX catalogs or within code.

    A test ID appearing more than once in the LaTeX catalog tables indicates
    a copy-paste error.  A test ID appearing in multiple code files indicates
    an ID collision that creates ambiguity about which test is which.

    Args:
        latex_ids: All test IDs extracted from all LaTeX catalog tables.
        code_id_files: Mapping from test ID to list of files containing it.

    Returns:
        A list of Diagnostic instances for any duplicates found.
    """
    diagnostics: list[Diagnostic] = []

    # Check for duplicates in LaTeX
    latex_counts = Counter(latex_ids)
    for test_id, count in sorted(latex_counts.items()):
        if count > 1:
            diagnostics.append(Diagnostic(
                level="error",
                category="test_id_duplicates",
                section="latex_catalogs",
                detail=(
                    f"test ID {test_id} appears {count} times in the "
                    f"LaTeX catalog tables"
                ),
            ))

    # Check for duplicates in code (same ID in multiple files)
    for test_id, files in sorted(code_id_files.items()):
        if len(files) > 1:
            file_list = ", ".join(files)
            diagnostics.append(Diagnostic(
                level="error",
                category="test_id_duplicates",
                section="code_tests",
                detail=(
                    f"test ID {test_id} appears in {len(files)} files: "
                    f"{file_list}"
                ),
            ))

    return diagnostics


def check_test_id_sequential(
    ids_by_prefix: dict[str, list[str]],
    source: str,
) -> list[Diagnostic]:
    """Check 2: Verify that test IDs within each prefix are sequential.

    For each prefix group, collects the numeric parts of all IDs (ignoring
    letter suffixes like ``024a``, ``024b``) and checks for gaps in the
    sequence.  Gaps indicate deleted or forgotten tests.

    This is reported as a warning (or info) rather than an error, since
    gaps arise naturally when test IDs are renumbered or tests are removed.

    Args:
        ids_by_prefix: Dict mapping prefix strings to their test ID lists.
        source: Either ``"latex"`` or ``"code"`` (for diagnostic messages).

    Returns:
        A list of Diagnostic instances (warnings) for any gaps found.
    """
    diagnostics: list[Diagnostic] = []

    for prefix, ids in sorted(ids_by_prefix.items()):
        # Collect unique numeric parts (ignore letter suffixes)
        numbers = sorted(set(_numeric_part(tid) for tid in ids))
        if not numbers:
            continue

        # Find gaps in the sequence
        gaps: list[int] = []
        for i in range(len(numbers) - 1):
            expected_next = numbers[i] + 1
            actual_next = numbers[i + 1]
            if actual_next > expected_next:
                for missing in range(expected_next, actual_next):
                    gaps.append(missing)

        if gaps:
            gap_str = ", ".join(str(n) for n in gaps[:10])
            if len(gaps) > 10:
                gap_str += f", ... ({len(gaps)} total)"
            diagnostics.append(Diagnostic(
                level="warning",
                category="test_id_sequential",
                section=f"{source}_tests",
                detail=(
                    f"T-{prefix} has gaps in numbering: missing "
                    f"{gap_str} (range {numbers[0]}-{numbers[-1]})"
                ),
            ))

    return diagnostics


def check_catalog_counts(
    catalogs: list[tuple[str, list[str], int | None]],
) -> list[Diagnostic]:
    """Check 3: Verify that each catalog table's entry count matches its caption.

    Each test catalog longtable has a ``\\caption`` that claims a specific
    number of tests (e.g., ``"42 tests across all library crates"``).  This
    check compares the actual number of test ID entries in the table body
    against the claimed count.

    Args:
        catalogs: A list of ``(catalog_name, test_ids, caption_count)``
            tuples where *catalog_name* is a label for the catalog,
            *test_ids* is the list of IDs parsed from the table body, and
            *caption_count* is the integer from the caption (or ``None``
            if not parseable).

    Returns:
        A list of Diagnostic instances for any mismatches.
    """
    diagnostics: list[Diagnostic] = []

    for name, ids, caption_count in catalogs:
        if caption_count is None:
            continue

        actual_count = len(ids)
        if actual_count != caption_count:
            diagnostics.append(Diagnostic(
                level="error",
                category="catalog_count",
                section=name,
                detail=(
                    f"caption claims {caption_count} tests but the table "
                    f"body contains {actual_count} entries"
                ),
            ))

    return diagnostics


def check_summary_arithmetic(summary: dict) -> list[Diagnostic]:
    """Check 4: Verify that the summary table's arithmetic is internally correct.

    Checks that:

    - The sum of all regular data rows equals the total
    - Each subtotal equals the sum of its constituent rows
    - The sum of all subtotals equals the total

    Args:
        summary: The parsed summary table dict from
            :func:`parse_summary_table`.

    Returns:
        A list of Diagnostic instances for any arithmetic errors.
    """
    diagnostics: list[Diagnostic] = []

    rows = summary["rows"]
    subtotals = summary["subtotals"]
    total = summary["total"]
    caption_total = summary["caption_total"]

    # Check that the sum of all regular rows equals the total
    if total is not None and rows:
        row_sum = sum(c for _, c, _ in rows)
        if row_sum != total:
            diagnostics.append(Diagnostic(
                level="error",
                category="summary_arithmetic",
                section="summary_table",
                detail=(
                    f"data rows sum to {row_sum} but the total "
                    f"row claims {total}"
                ),
            ))

    # Check that the sum of subtotals equals the total
    if total is not None and subtotals:
        subtotal_sum = sum(subtotals.values())
        if subtotal_sum != total:
            diagnostics.append(Diagnostic(
                level="error",
                category="summary_arithmetic",
                section="summary_table",
                detail=(
                    f"subtotals sum to {subtotal_sum} but the total "
                    f"row claims {total}"
                ),
            ))

    # Check that the body total matches the caption total
    if total is not None and caption_total is not None:
        if total != caption_total:
            diagnostics.append(Diagnostic(
                level="error",
                category="summary_arithmetic",
                section="summary_table",
                detail=(
                    f"summary table body total is {total} but the "
                    f"caption claims {caption_total}"
                ),
            ))

    return diagnostics


def check_summary_vs_catalog(
    summary: dict,
    crate_sections: dict[str, list[str]],
    all_latex_ids: list[str],
) -> list[Diagnostic]:
    """Check 5: Verify that summary per-crate counts match catalog entry counts.

    Compares the count claimed for each row in the summary table against
    the actual number of test entries in the corresponding catalog table
    section.

    Args:
        summary: The parsed summary table dict.
        crate_sections: Dict mapping crate names to their test ID lists
            from catalog tables.
        all_latex_ids: All test IDs from all catalog tables (for total
            comparison).

    Returns:
        A list of Diagnostic instances for any mismatches.
    """
    diagnostics: list[Diagnostic] = []

    for name, count, _ in summary["rows"]:
        # Try to match summary row to a crate section
        # Pattern: "neuronprompter-xxx" or "neuronprompter-xxx unit tests"
        crate_match = re.search(r"(neuronprompter[-\w]*)", name.lower())
        if not crate_match:
            continue

        crate_name = crate_match.group(1)

        # Find the matching crate section in the catalog
        catalog_count = None
        for catalog_crate, ids in crate_sections.items():
            if crate_name in catalog_crate.lower():
                catalog_count = len(ids)
                break

        if catalog_count is not None and catalog_count != count:
            diagnostics.append(Diagnostic(
                level="error",
                category="summary_vs_catalog",
                section="summary_table",
                detail=(
                    f'summary row "{name}" claims {count} tests but '
                    f"the catalog has {catalog_count} entries for "
                    f"{crate_name}"
                ),
            ))

    return diagnostics


def check_latex_tests_in_code(
    latex_ids: set[str],
    code_ids: set[str],
) -> list[Diagnostic]:
    """Check 6: Verify that every test ID in LaTeX catalogs exists in code.

    Test IDs documented in the architecture document should have
    corresponding test implementations in the Rust codebase.  A LaTeX-only
    test ID means the documented test was either not implemented or the ID
    was changed.

    Args:
        latex_ids: Set of all test IDs from LaTeX catalog tables.
        code_ids: Set of all test IDs from code doc comments.

    Returns:
        A list of Diagnostic instances for any LaTeX-only IDs.
    """
    diagnostics: list[Diagnostic] = []

    for test_id in sorted(latex_ids - code_ids):
        diagnostics.append(Diagnostic(
            level="error",
            category="latex_tests_in_code",
            section="test_coverage",
            detail=(
                f"test {test_id} is documented in the LaTeX catalog but "
                f"has no matching test function in the code"
            ),
        ))

    return diagnostics


def check_code_tests_in_latex(
    code_ids: set[str],
    latex_ids: set[str],
    code_id_files: dict[str, list[str]],
) -> list[Diagnostic]:
    """Check 7: Verify that every test ID in code exists in LaTeX catalogs.

    Test functions with ``T-XXX-NNN`` doc comment IDs should be documented
    in the corresponding LaTeX catalog table.  A code-only test ID means
    the test was added to code without updating the documentation.

    Args:
        code_ids: Set of all test IDs from code doc comments.
        latex_ids: Set of all test IDs from LaTeX catalog tables.
        code_id_files: Mapping from test ID to file paths (for messages).

    Returns:
        A list of Diagnostic instances for any code-only IDs.
    """
    diagnostics: list[Diagnostic] = []

    for test_id in sorted(code_ids - latex_ids):
        files = code_id_files.get(test_id, ["unknown"])
        file_str = files[0] if files else "unknown"
        diagnostics.append(Diagnostic(
            level="error",
            category="code_tests_in_latex",
            section="test_coverage",
            detail=(
                f"test {test_id} exists in code ({file_str}) but is "
                f"missing from the LaTeX catalog"
            ),
        ))

    return diagnostics


def check_fn_name_matches_doc_id(root: Path) -> list[Diagnostic]:
    """Check 8: Verify that each test function name's ID matches its doc comment.

    The doc-comment test ID (e.g., ``T-CORE-061``) is the canonical
    identifier used in the LaTeX documentation.  The function name follows
    the pattern ``t_{prefix}_{number}_{description}``, and the number
    portion must match the doc-comment ID.  A mismatch means the LaTeX
    traceability table points to the wrong function name.

    Args:
        root: The Cargo workspace root directory.

    Returns:
        A list of Diagnostic instances for any mismatches found.
    """
    crates_dir = root / "crates"
    diagnostics: list[Diagnostic] = []

    if not crates_dir.is_dir():
        return diagnostics

    doc_id_re = re.compile(
        r"///\s+(T-[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{3}[a-z]?):"
    )

    for rs_file in sorted(crates_dir.rglob("*.rs")):
        try:
            lines = rs_file.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeDecodeError):
            continue

        rel_path = str(rs_file.relative_to(root)).replace("\\", "/")

        pending_doc_id: str | None = None
        pending_line: int = 0
        countdown: int = 0

        for line_no, line in enumerate(lines, start=1):
            doc_match = doc_id_re.search(line)
            if doc_match:
                pending_doc_id = doc_match.group(1)
                pending_line = line_no
                countdown = 15
                continue

            if pending_doc_id is not None:
                countdown -= 1
                if countdown <= 0:
                    pending_doc_id = None
                    continue

                fn_match = _FN_DEF_RE.match(line)
                if fn_match:
                    fn_name = fn_match.group(1)
                    parsed = _fn_name_to_prefix_number(fn_name)
                    if parsed is not None:
                        fn_prefix, fn_number = parsed
                        doc_prefix, doc_number = _doc_id_to_prefix_number(
                            pending_doc_id,
                        )
                        if (
                            fn_prefix == doc_prefix
                            and fn_number != doc_number
                        ):
                            diagnostics.append(Diagnostic(
                                level="error",
                                category="fn_name_doc_id",
                                section="traceability",
                                detail=(
                                    f"{rel_path}:{pending_line} documents "
                                    f"{pending_doc_id}, but the test "
                                    f"function at line {line_no} is "
                                    f"{fn_name} (number {fn_number} != "
                                    f"{doc_number})"
                                ),
                            ))
                    pending_doc_id = None

    return diagnostics


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------

def format_human_readable(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as human-readable text lines.

    Each line has the format::

        [ERROR|WARN|INFO] <category>: <section> -- <detail>

    A summary line at the end reports total error, warning, and info counts.

    Args:
        diagnostics: The list of Diagnostic instances to format.

    Returns:
        A multi-line string with one diagnostic per line plus a summary.
    """
    lines: list[str] = []
    error_count = 0
    warning_count = 0
    info_count = 0

    for diag in diagnostics:
        if diag.level == "error":
            prefix = "[ERROR]"
            error_count += 1
        elif diag.level == "info":
            prefix = "[INFO]"
            info_count += 1
        else:
            prefix = "[WARN]"
            warning_count += 1

        lines.append(
            f"{prefix} {diag.category}: {diag.section} -- {diag.detail}",
        )

    summary = f"SUMMARY: {error_count} error(s), {warning_count} warning(s)"
    if info_count:
        summary += f", {info_count} info(s)"
    lines.append(summary)
    return "\n".join(lines)


def format_json(diagnostics: list[Diagnostic]) -> str:
    """Format a list of diagnostics as a JSON string.

    The JSON object has four keys: ``"errors"`` (list of error entries),
    ``"warnings"`` (list of warning entries), ``"infos"`` (list of info
    entries), and ``"summary"`` (counts).  Each entry has ``"category"``,
    ``"section"``, and ``"detail"`` fields.

    Args:
        diagnostics: The list of Diagnostic instances to format.

    Returns:
        A JSON string with 2-space indentation.
    """
    errors = []
    warnings = []
    infos = []

    for diag in diagnostics:
        entry = {
            "category": diag.category,
            "section": diag.section,
            "detail": diag.detail,
        }
        if diag.level == "error":
            errors.append(entry)
        elif diag.level == "info":
            infos.append(entry)
        else:
            warnings.append(entry)

    result = {
        "errors": errors,
        "warnings": warnings,
        "infos": infos,
        "summary": {
            "error_count": len(errors),
            "warning_count": len(warnings),
            "info_count": len(infos),
        },
    }

    return json.dumps(result, indent=2)


# ---------------------------------------------------------------------------
# Validation orchestration
# ---------------------------------------------------------------------------

def run_validation(tex_path: Path, root_path: Path) -> list[Diagnostic]:
    """Run all 8 validation checks and return the combined diagnostics.

    Reads the LaTeX document and scans the Rust source tree for test IDs.
    Parses all LaTeX catalog tables and the summary table, then executes
    each check function in order.

    Gracefully handles the bootstrap case where no test catalogs or test
    IDs exist yet.  When both sides are empty, no errors are reported.

    Args:
        tex_path: Absolute path to ``architecture.tex``.
        root_path: Absolute path to the Cargo workspace root.

    Returns:
        A combined list of Diagnostic instances from all checks.
    """
    raw_tex = tex_path.read_text(encoding="utf-8")
    tex_content = resolve_inputs(raw_tex, tex_path.parent)

    # ----- Parse LaTeX catalogs -----
    catalogs = parse_test_catalogs(tex_content)
    crate_sections = parse_crate_sections(tex_content)
    summary = parse_summary_table(tex_content)

    # Combine all LaTeX test IDs
    all_latex_ids: list[str] = []
    for _, ids, _ in catalogs:
        all_latex_ids.extend(ids)
    latex_id_set = set(all_latex_ids)

    # Group LaTeX IDs by prefix for sequential check
    latex_by_prefix: dict[str, list[str]] = {}
    for tid in all_latex_ids:
        prefix = _base_prefix(tid)
        if prefix not in latex_by_prefix:
            latex_by_prefix[prefix] = []
        latex_by_prefix[prefix].append(tid)

    # ----- Parse code test IDs -----
    code_by_prefix = parse_code_test_ids(root_path)
    code_id_files = map_code_test_files(root_path)

    # Flatten code IDs into a set (deduplicate across files)
    code_id_set: set[str] = set()
    for ids in code_by_prefix.values():
        code_id_set.update(ids)

    # ----- Bootstrap case: both empty -----
    # When there are no test IDs on either side, there is nothing to
    # validate.  Return early with no diagnostics.
    if not all_latex_ids and not code_id_set:
        return []

    # ----- Run all checks -----
    diagnostics: list[Diagnostic] = []

    # Check 1: Duplicate IDs
    diagnostics.extend(
        check_test_id_duplicates(all_latex_ids, code_id_files),
    )

    # Check 2: Sequential numbering (both LaTeX and code)
    diagnostics.extend(
        check_test_id_sequential(latex_by_prefix, "latex"),
    )
    diagnostics.extend(
        check_test_id_sequential(code_by_prefix, "code"),
    )

    # Check 3: Catalog caption counts
    diagnostics.extend(check_catalog_counts(catalogs))

    # Check 4: Summary arithmetic
    if summary:
        diagnostics.extend(check_summary_arithmetic(summary))

    # Check 5: Summary vs catalog
    if summary:
        diagnostics.extend(
            check_summary_vs_catalog(
                summary, crate_sections, all_latex_ids,
            ),
        )

    # Check 6: LaTeX IDs in code
    diagnostics.extend(
        check_latex_tests_in_code(latex_id_set, code_id_set),
    )

    # Check 7: Code IDs in LaTeX
    diagnostics.extend(
        check_code_tests_in_latex(code_id_set, latex_id_set, code_id_files),
    )

    # Check 8: Function name matches doc comment ID
    diagnostics.extend(check_fn_name_matches_doc_id(root_path))

    return diagnostics


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def main() -> int:
    """Parse command-line arguments, run validation, and produce output.

    Returns:
        Exit code: 0 if all checks pass, 1 if discrepancies found, 2 if
        the script cannot parse the LaTeX file or locate required source
        files.
    """
    parser = argparse.ArgumentParser(
        description=(
            "Test Catalog Validator for NeuronPrompter.  "
            "Checks bidirectional consistency between the LaTeX test "
            "catalog documentation and the Rust test code.  Detects "
            "missing tests, count mismatches, numbering inconsistencies, "
            "and arithmetic errors."
        ),
    )
    parser.add_argument(
        "--tex",
        required=True,
        type=Path,
        help="Path to the LaTeX architecture document",
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
            "Treat warnings as errors (used in CI to block the "
            "pipeline on any discrepancy)"
        ),
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON instead of human-readable text",
    )
    args = parser.parse_args()

    # Resolve paths and validate existence
    tex_path: Path = args.tex.resolve()
    root_path: Path = args.root.resolve()

    if not tex_path.is_file():
        print(f"Error: LaTeX file not found: {tex_path}", file=sys.stderr)
        return 2

    if not root_path.is_dir():
        print(
            f"Error: workspace root directory not found: {root_path}",
            file=sys.stderr,
        )
        return 2

    # Verify that the crates directory exists
    crates_dir = root_path / "crates"
    if not crates_dir.is_dir():
        print(
            f"Error: crates directory not found: {crates_dir}",
            file=sys.stderr,
        )
        return 2

    # Run validation
    try:
        diagnostics = run_validation(tex_path, root_path)
    except Exception as exc:
        print(f"Error: validation failed: {exc}", file=sys.stderr)
        return 2

    # Format and output results
    if args.json:
        print(format_json(diagnostics))
    else:
        if diagnostics:
            print(format_human_readable(diagnostics), file=sys.stderr)
        else:
            print("All checks passed.", file=sys.stderr)

    # Determine exit code
    error_count = sum(1 for d in diagnostics if d.level == "error")
    warning_count = sum(1 for d in diagnostics if d.level == "warning")

    if error_count > 0:
        return 1
    if args.strict and warning_count > 0:
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
