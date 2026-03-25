"""
Generates visual architecture diagrams for a Rust workspace codebase.

The crate dependency graph (diagram 1) and module file tree (diagram 5) are
fully automatic and work with any Rust workspace without modification.
Diagrams 2-4 and 6 contain project-specific content for NeuronPrompter.

Outputs (saved to docs/diagrams/):
  1. crate_dependencies.png  - Auto-generated inter-crate dependency graph
  2. architecture_layers.png - Layered architecture overview (project-specific)
  3. type_map.png            - Domain type relationships (project-specific)
  4. api_endpoint_map.png    - REST API endpoint structure (project-specific)
  5. module_tree.png         - Auto-generated source file tree per crate
  6. error_propagation.png   - Error type wrapping chain (project-specific)
  7. request_lifecycle.png   - HTTP request flow through layers (project-specific)

CLI flags:
  --curved-lines     Use curved edges instead of the default orthogonal (right-angle) lines.
  --no-optimize      Disable extra Graphviz layout iterations (faster but messier).
  --auto-only        Skip project-specific diagrams, generate only auto-detected ones.

Requires:
  - Graphviz CLI (dot) installed and on PATH
  - matplotlib (pip install matplotlib)
"""

import subprocess
import re
import sys
import textwrap
from pathlib import Path
from typing import Optional

# Output goes to docs/diagrams/ relative to the workspace root.
PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
DOCS_DIR = PROJECT_ROOT / "docs" / "diagrams"

# When True, all diagrams use only straight orthogonal lines instead of curves.
# Enabled by default; pass --curved-lines to disable.
STRAIGHT_LINES = "--curved-lines" not in sys.argv

# When True, Graphviz spends more iterations minimizing edge crossings and
# optimizing node placement. Produces cleaner layouts at the cost of compute time.
# Enabled by default; pass --no-optimize to disable.
OPTIMIZE = "--no-optimize" not in sys.argv

# When True, only auto-detected diagrams (crate deps, module tree) are generated.
AUTO_ONLY = "--auto-only" in sys.argv

# Palette of fill colors cycled through for auto-generated node coloring.
COLOR_PALETTE = [
    "#E8F5E9", "#E3F2FD", "#FFF3E0", "#FCE4EC", "#F3E5F5",
    "#E0F7FA", "#FFF9C4", "#FFECB3", "#E8EAF6", "#F1F8E9",
]


# ---------------------------------------------------------------------------
# Cargo.toml parsing (no toml library required — simple regex extraction)
# ---------------------------------------------------------------------------

def parse_workspace_members(cargo_toml_path: Path) -> list[str]:
    """
    Extract workspace member paths from a root Cargo.toml.
    Returns a list of relative directory paths like ["crates/foo", "crates/bar"].
    """
    text = cargo_toml_path.read_text(encoding="utf-8")
    members_match = re.search(r"members\s*=\s*\[(.*?)\]", text, re.DOTALL)
    if not members_match:
        return []
    block = members_match.group(1)
    return re.findall(r'"([^"]+)"', block)


def parse_crate_name(cargo_toml_path: Path) -> Optional[str]:
    """Extract the [package] name from a crate-level Cargo.toml."""
    text = cargo_toml_path.read_text(encoding="utf-8")
    match = re.search(r'^\s*name\s*=\s*"([^"]+)"', text, re.MULTILINE)
    return match.group(1) if match else None


def parse_internal_deps(cargo_toml_path: Path, all_crate_names: set[str]) -> list[str]:
    """
    Extract dependency names from a crate-level Cargo.toml that are also
    workspace members (internal dependencies). Handles both inline and
    table-style dependency declarations, plus workspace = true references.
    """
    text = cargo_toml_path.read_text(encoding="utf-8")
    deps = set()

    # Match lines like: some-crate = { path = "..." } or some-crate.workspace = true
    # or [dependencies.some-crate] sections
    for name in all_crate_names:
        # Pattern: name = ... (anywhere in [dependencies] or [dev-dependencies])
        escaped = re.escape(name)
        if re.search(rf"(?:^|\n)\s*{escaped}\s*=", text):
            deps.add(name)
        # Pattern: [dependencies.name] or [dev-dependencies.name]
        if re.search(rf"\[.*dependencies\.{escaped}\]", text):
            deps.add(name)

    return sorted(deps)


def discover_workspace(root: Path) -> dict[str, list[str]]:
    """
    Auto-discover all workspace crates and their internal dependencies.
    Returns a dict mapping crate_name -> [list of internal dependency names].
    """
    root_cargo = root / "Cargo.toml"
    if not root_cargo.exists():
        return {}

    member_paths = parse_workspace_members(root_cargo)
    if not member_paths:
        return {}

    # First pass: collect all crate names
    crate_info: dict[str, Path] = {}
    for member_rel in member_paths:
        member_cargo = root / member_rel / "Cargo.toml"
        if member_cargo.exists():
            name = parse_crate_name(member_cargo)
            if name:
                crate_info[name] = member_cargo

    all_names = set(crate_info.keys())

    # Second pass: resolve internal dependencies
    result: dict[str, list[str]] = {}
    for name, cargo_path in crate_info.items():
        result[name] = parse_internal_deps(cargo_path, all_names - {name})

    return result


# ---------------------------------------------------------------------------
# Source file tree discovery
# ---------------------------------------------------------------------------

def discover_source_tree(root: Path) -> dict[str, list[str]]:
    """
    For each workspace crate, collect all .rs source file paths relative to the
    crate directory. Returns crate_name -> sorted list of relative paths.
    """
    member_paths = parse_workspace_members(root / "Cargo.toml")
    result: dict[str, list[str]] = {}

    for member_rel in member_paths:
        member_dir = root / member_rel
        cargo_path = member_dir / "Cargo.toml"
        if not cargo_path.exists():
            continue
        name = parse_crate_name(cargo_path)
        if not name:
            continue

        rs_files = sorted(
            str(p.relative_to(member_dir)).replace("\\", "/")
            for p in member_dir.rglob("*.rs")
            if "target" not in p.parts
        )
        if rs_files:
            result[name] = rs_files

    return result


# ---------------------------------------------------------------------------
# DOT rendering helpers
# ---------------------------------------------------------------------------

def _apply_dot_options(dot_source: str) -> str:
    """
    Modify DOT source based on active CLI flags before rendering.

    STRAIGHT_LINES: Insert splines=ortho and convert edge label= to xlabel=
    since ortho mode does not support inline edge labels.

    OPTIMIZE: Insert layout tuning parameters that increase the number of
    iterations the dot engine uses for crossing minimization and node placement.
    - mclimit=200:     Run crossing minimization 200x longer than default.
    - nslimit=200:     Run network simplex (ranking) 200x longer.
    - nslimit1=200:    Run network simplex (positioning) 200x longer.
    - remincross=true: Re-run crossing minimization a second time.
    - searchsize=2000: Broader search in network simplex (default 30).
    """
    injections = []

    if OPTIMIZE:
        injections.append(
            "    mclimit=200;\n"
            "    nslimit=200;\n"
            "    nslimit1=200;\n"
            "    remincross=true;\n"
            "    searchsize=2000;"
        )

    if STRAIGHT_LINES:
        injections.append("    splines=ortho;")

    if injections:
        block = "\n".join(injections)
        dot_source = dot_source.replace("{\n", "{\n" + block + "\n", 1)

    if STRAIGHT_LINES:
        dot_source = dot_source.replace("[label=", "[xlabel=")
        # Restore node labels that were incorrectly converted to xlabel.
        # Node definitions contain fillcolor or shape= attributes.
        lines = dot_source.split("\n")
        fixed = []
        for line in lines:
            stripped = line.strip()
            if ("fillcolor=" in stripped or "shape=" in stripped) and "xlabel=" in stripped:
                line = line.replace("xlabel=", "label=")
            fixed.append(line)
        dot_source = "\n".join(fixed)

    return dot_source


def write_dot_and_render(dot_source: str, output_name: str,
                         override_splines: str | None = None) -> None:
    """Write a DOT file and render it to PNG via the Graphviz dot command."""
    dot_source = _apply_dot_options(dot_source)
    if override_splines:
        # Replace the injected splines=ortho with a per-diagram override
        dot_source = dot_source.replace("splines=ortho;", f"splines={override_splines};")
    dot_path = DOCS_DIR / f"{output_name}.dot"
    png_path = DOCS_DIR / f"{output_name}.png"

    dot_path.write_text(dot_source, encoding="utf-8")

    subprocess.run(
        ["dot", "-Tpng", "-Gdpi=200", str(dot_path), "-o", str(png_path)],
        check=True,
    )
    dot_path.unlink()
    print(f"  -> {png_path}")


def _sanitize_id(name: str) -> str:
    """Convert a crate name like 'neuronprompter-core' to a valid DOT node id."""
    return name.replace("-", "_")


# ---------------------------------------------------------------------------
# Diagram 1: Auto-generated crate dependency graph
# ---------------------------------------------------------------------------

def generate_crate_dependency_graph() -> None:
    """
    Auto-detect workspace crates and their internal dependencies from Cargo.toml
    files, then render the dependency graph. Works with any Rust workspace.
    """
    workspace = discover_workspace(PROJECT_ROOT)
    if not workspace:
        print("  [skip] No workspace members found")
        return

    # Determine project title from root directory name
    project_title = PROJECT_ROOT.name

    lines = [
        "digraph crate_dependencies {",
        '    rankdir=BT;',
        '    fontname="Helvetica";',
        '    node [shape=box, style="filled,rounded", fontname="Helvetica", fontsize=11, margin="0.2,0.1"];',
        '    edge [color="#555555", arrowsize=0.7];',
        '    bgcolor="white";',
        f'    label="{project_title} — Crate Dependency Graph";',
        '    labelloc=t;',
        '    fontsize=16;',
        '',
    ]

    # Sort crates by dependency count (fewest deps first = bottom of graph)
    sorted_crates = sorted(workspace.keys(), key=lambda c: len(workspace[c]))

    # Assign colors from palette
    for i, crate_name in enumerate(sorted_crates):
        node_id = _sanitize_id(crate_name)
        color = COLOR_PALETTE[i % len(COLOR_PALETTE)]
        lines.append(f'    {node_id} [label="{crate_name}", fillcolor="{color}"];')

    lines.append('')

    # Edges
    for crate_name, deps in workspace.items():
        src = _sanitize_id(crate_name)
        for dep in deps:
            dst = _sanitize_id(dep)
            lines.append(f'    {src} -> {dst};')

    lines.append('}')
    write_dot_and_render("\n".join(lines), "crate_dependencies")


# ---------------------------------------------------------------------------
# Diagram 5: Auto-generated module / file tree
# ---------------------------------------------------------------------------

def generate_module_tree() -> None:
    """
    Auto-detect all .rs source files per crate and render a file tree diagram
    using matplotlib for a clean grid-based layout.
    Works with any Rust workspace.
    """
    tree = discover_source_tree(PROJECT_ROOT)
    if not tree:
        print("  [skip] No source files found")
        return

    project_title = PROJECT_ROOT.name

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.patches as mpatches

    sorted_crates = sorted(tree.keys())
    n = len(sorted_crates)
    cols = 4
    rows = (n + cols - 1) // cols

    cell_w = 3.5
    cell_h_base = 0.8  # header height
    line_h = 0.16      # per file line

    # Compute max cell height for uniform sizing
    max_files = 15
    max_cell_h = cell_h_base + max_files * line_h + 0.3

    fig_w = cols * cell_w + 1.0
    fig_h = rows * (max_cell_h + 0.4) + 1.5

    fig, ax = plt.subplots(figsize=(fig_w, fig_h))
    ax.set_xlim(0, fig_w)
    ax.set_ylim(0, fig_h)
    ax.axis("off")
    fig.patch.set_facecolor("white")

    ax.text(fig_w / 2, fig_h - 0.5, f"{project_title} — Source File Tree",
            ha="center", va="center", fontsize=14, fontweight="bold",
            fontfamily="Helvetica")

    for idx, crate_name in enumerate(sorted_crates):
        col = idx % cols
        row = idx // cols

        x = 0.5 + col * cell_w
        y = fig_h - 1.3 - row * (max_cell_h + 0.4)

        color = COLOR_PALETTE[idx % len(COLOR_PALETTE)]
        files = tree[crate_name]
        src_files = [f for f in files if f.startswith("src/")]
        if not src_files:
            src_files = files

        display_files = src_files[:max_files]
        n_files = len(src_files)
        cell_h = cell_h_base + len(display_files) * line_h + 0.3
        if n_files > max_files:
            cell_h += line_h

        rect = mpatches.FancyBboxPatch(
            (x, y - cell_h), cell_w - 0.3, cell_h,
            boxstyle="round,pad=0.08",
            facecolor=color, edgecolor="#666666", linewidth=0.8,
        )
        ax.add_patch(rect)

        # Crate name header
        ax.text(x + 0.15, y - 0.2, f"{crate_name}  ({n_files} files)",
                fontsize=8, fontweight="bold", va="top", fontfamily="Helvetica")

        # File list
        for fi, fname in enumerate(display_files):
            ax.text(x + 0.2, y - 0.5 - fi * line_h, fname,
                    fontsize=6, va="top", fontfamily="Helvetica", color="#333333")

        if n_files > max_files:
            ax.text(x + 0.2, y - 0.5 - len(display_files) * line_h,
                    f"... +{n_files - max_files} more",
                    fontsize=6, va="top", fontfamily="Helvetica", color="#888888",
                    style="italic")

    out = DOCS_DIR / "module_tree.png"
    fig.savefig(str(out), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  -> {out}")


# ---------------------------------------------------------------------------
# Diagram 6: Error propagation chain (project-specific)
# ---------------------------------------------------------------------------

def generate_error_propagation() -> None:
    """
    Render the error type wrapping and mapping chain across crate boundaries.
    Shows how domain errors propagate through service, API, and MCP layers.
    """
    dot = textwrap.dedent("""\
    digraph error_propagation {
        rankdir=LR;
        fontname="Helvetica";
        node [fontname="Helvetica", fontsize=10, shape=record, style=filled, margin="0.15,0.07"];
        edge [arrowsize=0.6, fontsize=9, fontname="Helvetica"];
        bgcolor="white";
        label="NeuronPrompter — Error Propagation Chain";
        labelloc=t;
        fontsize=14;
        nodesep=0.5;
        ranksep=1.2;

        // Source errors
        CoreError [fillcolor="#FFCDD2",
            label="{CoreError\\n(neuronprompter-core)|Validation\\lNotFound\\lDuplicate\\lPromptInUse\\lScriptInUse}"];

        RusqliteError [fillcolor="#E0E0E0",
            label="{rusqlite::Error\\n(external)}"];

        DbError [fillcolor="#BBDEFB",
            label="{DbError\\n(neuronprompter-db)|operation: String\\lsource: rusqlite::Error}"];

        // Service layer
        ServiceError [fillcolor="#FFE0B2",
            label="{ServiceError\\n(neuronprompter-application)|Core(CoreError)\\lDatabase(DbError)\\lOllamaUnavailable\\lOllamaError(String)\\lIoError(io::Error)\\lSerializationError(String)}"];

        // API layer
        ApiError [fillcolor="#F8BBD0",
            label="{ApiError\\n(neuronprompter-api)|Validation \\u2192 400\\lNotFound \\u2192 404\\lDuplicate \\u2192 409\\lInUse \\u2192 409\\lDatabase \\u2192 500\\lOllama \\u2192 502}"];

        // MCP layer
        McpError [fillcolor="#E1BEE7",
            label="{McpError\\n(neuronprompter-mcp)|Validation \\u2192 -32602\\lNotFound \\u2192 -32602\\lDuplicate \\u2192 -32602\\lDatabase \\u2192 -32603\\lOllama \\u2192 -32600\\lIo \\u2192 -32603}"];

        // HTTP / JSON-RPC output
        HttpResponse [fillcolor="#C8E6C9",
            label="{HTTP Response|status: u16\\lbody: JSON\\lerror_code: String}"];

        JsonRpcError [fillcolor="#C8E6C9",
            label="{JSON-RPC Error|code: i32\\lmessage: String\\ldata: Option}"];

        // Wrapping edges
        RusqliteError -> DbError [label="wraps"];
        CoreError -> ServiceError [label="From"];
        DbError -> ServiceError [label="From"];
        ServiceError -> ApiError [label="From"];
        ServiceError -> McpError [label="map_err"];
        ApiError -> HttpResponse [label="IntoResponse"];
        McpError -> JsonRpcError [label="to_json_rpc"];
    }
    """)
    write_dot_and_render(dot, "error_propagation")


# ---------------------------------------------------------------------------
# Diagram 7: Request lifecycle (project-specific)
# ---------------------------------------------------------------------------

def generate_request_lifecycle() -> None:
    """
    Render the lifecycle of an HTTP request through all architectural layers.
    Shows the call chain from browser to database and back.
    """
    dot = textwrap.dedent("""\
    digraph request_lifecycle {
        rankdir=LR;
        fontname="Helvetica";
        node [fontname="Helvetica", fontsize=10, margin="0.15,0.07"];
        edge [fontname="Helvetica", fontsize=9, arrowsize=0.6];
        bgcolor="white";
        label="NeuronPrompter — Request Lifecycle";
        labelloc=t;
        fontsize=14;
        nodesep=0.6;
        ranksep=0.9;

        // Actors
        browser [label="Browser /\\nSolidJS SPA", shape=oval, style=filled, fillcolor="#E8EAF6"];
        mcp_client [label="MCP Client\\n(Claude Code)", shape=oval, style=filled, fillcolor="#F3E5F5"];

        // Middleware
        middleware [label="{Middleware|Rate Limiter\\lCORS\\lAuth (bearer token)}", shape=record, style=filled, fillcolor="#FFF9C4"];

        // Handler
        handler [label="{Handler\\n(neuronprompter-api)|Deserialize request\\lExtract AppState\\lCall service\\lSerialize response}", shape=record, style=filled, fillcolor="#FCE4EC"];

        // MCP transport
        mcp_transport [label="{MCP Transport\\n(neuronprompter-mcp)|stdio JSON-RPC\\lParse method + params\\lDispatch to tool handler}", shape=record, style=filled, fillcolor="#F3E5F5"];

        // Service
        service [label="{Service\\n(neuronprompter-application)|Validate input\\lBegin transaction\\lCall repositories\\lCreate version snapshot\\lSync associations\\lCommit}", shape=record, style=filled, fillcolor="#FFF3E0"];

        // Repository
        repo [label="{Repository\\n(neuronprompter-db)|Build SQL query\\lBind parameters\\lExecute statement\\lMap rows to domain types}", shape=record, style=filled, fillcolor="#E3F2FD"];

        // Database
        db [label="SQLite\\n(WAL mode)", shape=cylinder, style=filled, fillcolor="#E8F5E9"];

        // SSE
        sse [label="SSE Broadcast\\n(log + model events)", shape=oval, style=filled, fillcolor="#E0F7FA"];

        // HTTP path
        browser -> middleware [label="HTTP\\nrequest"];
        middleware -> handler [label="routed"];
        handler -> service [label="call"];
        service -> repo [label="query"];
        repo -> db [label="SQL"];

        // MCP path
        mcp_client -> mcp_transport [label="JSON-RPC\\nstdio"];
        mcp_transport -> service [label="call"];

        // Side effects
        handler -> sse [label="broadcast", style=dashed];
    }
    """)
    write_dot_and_render(dot, "request_lifecycle")


# ---------------------------------------------------------------------------
# Diagram 2: Architecture layers (project-specific, matplotlib)
# ---------------------------------------------------------------------------

def generate_architecture_layers() -> None:
    """
    Render a layered architecture overview using matplotlib.
    Shows the vertical stack from database to UI, plus the MCP sidecar.
    """
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.patches as mpatches

    fig, ax = plt.subplots(figsize=(14, 10))
    ax.set_xlim(0, 14)
    ax.set_ylim(0, 10)
    ax.axis("off")
    fig.patch.set_facecolor("white")

    project_title = PROJECT_ROOT.name

    # Title
    ax.text(7, 9.6, f"{project_title} — Architecture Overview",
            ha="center", va="center", fontsize=16, fontweight="bold",
            fontfamily="Helvetica")

    # Layer definitions: (y_bottom, height, color, label, details)
    layers = [
        (0.3, 1.2, "#E8F5E9", "neuronprompter-core",
         "Domain types  |  Repository traits  |  Service traits\n"
         "Validation  |  Template engine  |  CoreError"),
        (1.8, 1.2, "#E3F2FD", "neuronprompter-db",
         "SQLite via rusqlite  |  r2d2 pool  |  WAL mode\n"
         "Schema migrations  |  FTS5 search  |  12 repo modules"),
        (3.3, 1.2, "#FFF3E0", "neuronprompter-application",
         "Service implementations  |  Ollama LLM client\n"
         "Version snapshots  |  Association sync  |  Script filesystem sync"),
        (4.8, 1.2, "#FCE4EC", "neuronprompter-api",
         "Axum REST  |  40+ endpoints under /api/v1/\n"
         "AppState  |  Rate limiting  |  Error mapping to HTTP"),
        (6.3, 1.2, "#E0F7FA", "neuronprompter-web",
         "Embedded SolidJS SPA  |  SSE streams (logs + models)\n"
         "MCP registration UI  |  Ollama management  |  Native window (tao/wry)"),
        (7.8, 1.0, "#FFF9C4", "neuronprompter (binary)",
         "CLI dispatch  |  3 modes: web / serve / mcp\n"
         "Tokio runtime  |  Graceful shutdown"),
    ]

    main_left = 1.0
    main_width = 8.5

    for y, h, color, label, details in layers:
        rect = mpatches.FancyBboxPatch(
            (main_left, y), main_width, h,
            boxstyle="round,pad=0.1",
            facecolor=color, edgecolor="#666666", linewidth=1.2,
        )
        ax.add_patch(rect)
        ax.text(main_left + 0.3, y + h - 0.3, label,
                fontsize=11, fontweight="bold", va="top", fontfamily="Helvetica")
        ax.text(main_left + 0.3, y + 0.15, details,
                fontsize=8, va="bottom", fontfamily="Helvetica", color="#333333")

    # MCP sidecar
    mcp_rect = mpatches.FancyBboxPatch(
        (10.2, 0.3), 3.2, 5.7,
        boxstyle="round,pad=0.1",
        facecolor="#F3E5F5", edgecolor="#666666", linewidth=1.2,
    )
    ax.add_patch(mcp_rect)
    ax.text(11.8, 5.7, "neuronprompter-mcp",
            ha="center", fontsize=11, fontweight="bold", fontfamily="Helvetica")
    ax.text(11.8, 5.2, "(JSON-RPC / stdio)",
            ha="center", fontsize=9, fontfamily="Helvetica", color="#555555")

    mcp_details = (
        "18 tools exposed\n"
        "Read: prompts, chains,\n"
        "  scripts, taxonomy\n"
        "Write: prompts, chains\n\n"
        "Dedicated mcp_agent user\n"
        "No delete operations\n"
        "No Ollama access\n\n"
        "Clients:\n"
        "  Claude Code\n"
        "  Claude Desktop"
    )
    ax.text(11.8, 4.6, mcp_details,
            ha="center", va="top", fontsize=8, fontfamily="Helvetica", color="#333333")

    # Frontend callout
    fe_rect = mpatches.FancyBboxPatch(
        (10.2, 6.3), 3.2, 1.2,
        boxstyle="round,pad=0.1",
        facecolor="#E8EAF6", edgecolor="#666666", linewidth=1.2,
    )
    ax.add_patch(fe_rect)
    ax.text(11.8, 7.2, "SolidJS Frontend",
            ha="center", fontsize=11, fontweight="bold", fontfamily="Helvetica")
    ax.text(11.8, 6.45, "TypeScript  |  Vite  |  SSE client\n"
            "Prompt/Chain/Script editors\nOllama panel  |  Version history",
            ha="center", va="bottom", fontsize=8, fontfamily="Helvetica", color="#333333")

    # Arrows between layers (vertical, already straight by default)
    arrow_style = dict(arrowstyle="->", color="#888888", lw=1.5)
    mid_x = main_left + main_width / 2

    for i in range(len(layers) - 1):
        y_top = layers[i][0] + layers[i][1]
        y_bot = layers[i + 1][0]
        ax.annotate("", xy=(mid_x, y_bot), xytext=(mid_x, y_top),
                     arrowprops=arrow_style)

    # Side arrow style uses right-angle connectors when STRAIGHT_LINES is active
    side_arrow_base = dict(arrowstyle="->", color="#888888", lw=1.2, ls="--")
    if STRAIGHT_LINES:
        side_arrow_base["connectionstyle"] = "angle3,angleA=0,angleB=90"
    side_arrow = side_arrow_base

    # Arrow from web to MCP
    ax.annotate("", xy=(10.2, 4.5), xytext=(main_left + main_width, 5.4),
                arrowprops=side_arrow)

    # Arrow from web to frontend
    ax.annotate("", xy=(10.2, 6.9), xytext=(main_left + main_width, 6.9),
                arrowprops=side_arrow)
    ax.text(9.8, 7.05, "embeds", fontsize=8, ha="center", color="#888888",
            fontfamily="Helvetica")

    # Arrow from MCP to application
    ax.annotate("", xy=(main_left + main_width, 3.9),
                xytext=(10.2, 3.0),
                arrowprops=side_arrow)
    ax.text(9.8, 3.2, "uses", fontsize=8, ha="center", color="#888888",
            fontfamily="Helvetica")

    out = DOCS_DIR / "architecture_layers.png"
    fig.savefig(str(out), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  -> {out}")


# ---------------------------------------------------------------------------
# Diagram 3: Domain type map (project-specific)
# ---------------------------------------------------------------------------

def generate_type_map() -> None:
    """
    Render the domain type relationship map.
    Shows structs, enums, traits and their associations within neuronprompter-core.
    """
    dot = textwrap.dedent("""\
    digraph type_map {
        rankdir=LR;
        fontname="Helvetica";
        node [fontname="Helvetica", fontsize=10, margin="0.15,0.07"];
        edge [arrowsize=0.6, fontsize=8, fontname="Helvetica"];
        bgcolor="white";
        label="NeuronPrompter — Domain Type Map (neuronprompter-core)";
        labelloc=t;
        fontsize=14;
        compound=true;
        nodesep=0.4;
        ranksep=0.8;

        // --- Domain Entities ---
        subgraph cluster_entities {
            label="Domain Entities";
            style="dashed";
            color="#999999";

            User            [shape=record, style=filled, fillcolor="#E8F5E9",
                             label="{User|id: i64\\lusername: String\\ldisplay_name: Option\\lcreated_at: DateTime}"];
            Prompt          [shape=record, style=filled, fillcolor="#E3F2FD",
                             label="{Prompt|id: i64\\luser_id: i64\\ltitle: String\\lcontent: String\\ldescription: Option\\llanguage: Option\\lis_favorite: bool\\lis_archived: bool\\lversion: i32}"];
            Script          [shape=record, style=filled, fillcolor="#E3F2FD",
                             label="{Script|id: i64\\luser_id: i64\\ltitle: String\\lcontent: String\\llanguage: String\\lis_favorite: bool\\lis_archived: bool\\lversion: i32}"];
            Chain           [shape=record, style=filled, fillcolor="#FFF3E0",
                             label="{Chain|id: i64\\luser_id: i64\\ltitle: String\\lseparator: String\\lis_favorite: bool\\lis_archived: bool}"];
            ChainStep       [shape=record, style=filled, fillcolor="#FFF3E0",
                             label="{ChainStep|chain_id: i64\\lstep_type: String\\lprompt_id: Option\\lscript_id: Option\\lposition: i32}"];

            Tag             [shape=record, style=filled, fillcolor="#F3E5F5",
                             label="{Tag|id: i64\\luser_id: i64\\lname: String}"];
            Category        [shape=record, style=filled, fillcolor="#F3E5F5",
                             label="{Category|id: i64\\luser_id: i64\\lname: String}"];
            Collection      [shape=record, style=filled, fillcolor="#F3E5F5",
                             label="{Collection|id: i64\\luser_id: i64\\lname: String}"];

            PromptVersion   [shape=record, style=filled, fillcolor="#E0F7FA",
                             label="{PromptVersion|id: i64\\lprompt_id: i64\\lversion_number: i32\\ltitle: String\\lcontent: String}"];
            ScriptVersion   [shape=record, style=filled, fillcolor="#E0F7FA",
                             label="{ScriptVersion|id: i64\\lscript_id: i64\\lversion_number: i32\\ltitle: String\\lcontent: String}"];

            AppSetting      [shape=record, style=filled, fillcolor="#FFF9C4",
                             label="{AppSetting|key: String\\lvalue: String}"];
            UserSettings    [shape=record, style=filled, fillcolor="#FFF9C4",
                             label="{UserSettings|user_id: i64\\ltheme: Theme\\lsort_field: SortField\\lsort_direction: SortDirection}"];
        }

        // --- Enums ---
        subgraph cluster_enums {
            label="Enums";
            style="dashed";
            color="#999999";

            Theme          [shape=record, style=filled, fillcolor="#FFECB3",
                            label="{\\<\\<enum\\>\\> Theme|Light\\lDark\\lSystem}"];
            SortField      [shape=record, style=filled, fillcolor="#FFECB3",
                            label="{\\<\\<enum\\>\\> SortField|UpdatedAt\\lCreatedAt\\lTitle}"];
            SortDirection  [shape=record, style=filled, fillcolor="#FFECB3",
                            label="{\\<\\<enum\\>\\> SortDirection|Asc\\lDesc}"];
            CoreError      [shape=record, style=filled, fillcolor="#FFCDD2",
                            label="{\\<\\<enum\\>\\> CoreError|Validation\\lNotFound\\lDuplicate\\lPromptInUse\\lScriptInUse}"];
        }

        // --- Relationships ---
        Prompt       -> User          [label="belongs_to", style=dashed];
        Script       -> User          [label="belongs_to", style=dashed];
        Chain        -> User          [label="belongs_to", style=dashed];
        Tag          -> User          [label="belongs_to", style=dashed];
        Category     -> User          [label="belongs_to", style=dashed];
        Collection   -> User          [label="belongs_to", style=dashed];

        ChainStep    -> Chain         [label="part_of"];
        ChainStep    -> Prompt        [label="references", style=dashed];
        ChainStep    -> Script        [label="references", style=dashed];

        PromptVersion -> Prompt       [label="version_of"];
        ScriptVersion -> Script       [label="version_of"];

        Prompt       -> Tag           [label="many-to-many", dir=both, style=dotted];
        Prompt       -> Category      [label="many-to-many", dir=both, style=dotted];
        Prompt       -> Collection    [label="many-to-many", dir=both, style=dotted];

        UserSettings -> User          [label="per_user", style=dashed];
        UserSettings -> Theme         [label="uses", style=dotted];
        UserSettings -> SortField     [label="uses", style=dotted];
        UserSettings -> SortDirection [label="uses", style=dotted];
    }
    """)
    write_dot_and_render(dot, "type_map")


# ---------------------------------------------------------------------------
# Diagram 4: API endpoint map (project-specific)
# ---------------------------------------------------------------------------

def generate_api_endpoint_map() -> None:
    """
    Render the REST API endpoint structure grouped by resource.
    """
    dot = textwrap.dedent("""\
    digraph api_endpoints {
        rankdir=LR;
        fontname="Helvetica";
        node [fontname="Helvetica", fontsize=9, shape=record, style=filled];
        edge [arrowsize=0.5];
        bgcolor="white";
        label="NeuronPrompter — REST API Endpoint Map (/api/v1/)";
        labelloc=t;
        fontsize=14;
        nodesep=0.3;

        router [label="Router\\n/api/v1/", fillcolor="#FFF9C4", shape=oval];

        health [label="{Health|GET /health\\lPOST /shutdown}", fillcolor="#E8F5E9"];
        users  [label="{Users|GET /users\\lPOST /users\\lPOST /users/switch\\lDELETE /users/:id}", fillcolor="#E3F2FD"];

        prompts [label="{Prompts|GET /prompts\\lGET /prompts/:id\\lPOST /prompts\\lPUT /prompts/:id\\lDELETE /prompts/:id\\lPOST /prompts/:id/duplicate\\lPUT /prompts/:id/favorite\\lPUT /prompts/:id/archive}", fillcolor="#E3F2FD"];

        scripts [label="{Scripts|GET /scripts\\lGET /scripts/:id\\lPOST /scripts\\lPUT /scripts/:id\\lDELETE /scripts/:id\\lPOST /scripts/:id/duplicate\\lPOST /scripts/sync\\lPOST /scripts/import-file}", fillcolor="#E3F2FD"];

        chains [label="{Chains|GET /chains\\lGET /chains/:id\\lPOST /chains\\lPUT /chains/:id\\lDELETE /chains/:id\\lPOST /chains/:id/duplicate\\lGET /chains/:id/content\\lGET /chains/for-prompt/:id}", fillcolor="#FFF3E0"];

        taxonomy [label="{Taxonomy|GET,POST /tags\\lDELETE /tags/:id\\lGET,POST /categories\\lDELETE /categories/:id\\lGET,POST /collections\\lDELETE /collections/:id}", fillcolor="#F3E5F5"];

        versions [label="{Versions|GET /versions/prompt/:id\\lGET /versions/prompt/:id/:num\\lPOST /versions/prompt/:id/:num/restore\\l\\lGET /script-versions/script/:id\\lPOST .../restore}", fillcolor="#E0F7FA"];

        io [label="{Import/Export|POST /io/export/json\\lPOST /io/export/markdown\\lPOST /io/import/json\\lPOST /io/import/markdown\\lPOST /io/backup}", fillcolor="#FCE4EC"];

        clipboard [label="{Clipboard|POST /clipboard/copy\\lPOST /clipboard/copy-substituted\\lGET /clipboard/history}", fillcolor="#FCE4EC"];

        settings [label="{Settings|GET /settings/app/:key\\lPUT /settings/app/:key\\lGET /settings/user/:id\\lPUT /settings/user/:id}", fillcolor="#FFF9C4"];

        ollama [label="{Ollama|GET /ollama/status\\lPOST /ollama/improve\\lPOST /ollama/translate\\lPOST /ollama/autofill}", fillcolor="#FFECB3"];

        search [label="{Search|GET /search}", fillcolor="#FFECB3"];

        sse [label="{SSE Streams|GET /events/logs\\lGET /events/models}", fillcolor="#E0F7FA"];

        web_only [label="{Web-Only|GET /web/setup/status\\lPOST /web/setup/complete\\lGET /web/doctor/probes\\lGET /web/mcp/status\\lPOST /web/mcp/:target/install\\lPOST /web/mcp/:target/uninstall\\lGET /web/ollama/status\\lGET /web/ollama/models\\lGET /web/ollama/running\\l...}", fillcolor="#E0F7FA"];

        router -> health;
        router -> users;
        router -> prompts;
        router -> scripts;
        router -> chains;
        router -> taxonomy;
        router -> versions;
        router -> io;
        router -> clipboard;
        router -> settings;
        router -> ollama;
        router -> search;
        router -> sse;
        router -> web_only;
    }
    """)
    write_dot_and_render(dot, "api_endpoint_map")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    DOCS_DIR.mkdir(parents=True, exist_ok=True)

    print("Generating architecture diagrams...")
    if OPTIMIZE:
        print("  [optimize] mclimit=200, nslimit=200, searchsize=2000")
    if STRAIGHT_LINES:
        print("  [straight-lines] splines=ortho")
    print()

    # Auto-detected diagrams (work with any Rust workspace)
    print("[1/7] Crate dependency graph (auto-detected)")
    generate_crate_dependency_graph()

    print("[5/7] Module file tree (auto-detected)")
    generate_module_tree()

    if AUTO_ONLY:
        print()
        print("Done (auto-only mode). Skipped project-specific diagrams.")
        sys.exit(0)

    # Project-specific diagrams
    print("[2/7] Architecture layers")
    generate_architecture_layers()

    print("[3/7] Domain type map")
    generate_type_map()

    print("[4/7] API endpoint map")
    generate_api_endpoint_map()

    print("[6/7] Error propagation chain")
    generate_error_propagation()

    print("[7/7] Request lifecycle")
    generate_request_lifecycle()

    print()
    print(f"Done. All diagrams saved to {DOCS_DIR}")
