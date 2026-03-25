"""Generates the NeuronPrompter application icon as a bold "P" with the
brand's purple-to-cyan gradient on a transparent background.

The "P" is rendered from a bold sans-serif system font, centered on the
canvas, and filled with the brand gradient. This produces a clean,
recognizable letterform at all target sizes.

The gradient uses the brand colors #a855f7 (purple) and #22d3ee (cyan) at
135 degrees, centered on the P shape's bounding box so the exact midpoint
color falls at the geometric center of the icon.

Output files:
  - crates/neuronprompter/assets/icon.ico          Windows executable icon (16..256px)
  - crates/neuronprompter/assets/icon.icns         macOS .app bundle icon (128..1024px)
  - crates/neuronprompter-web/assets/icon_256.png   tao window icon (embedded via include_bytes!)
  - crates/neuronprompter-web/frontend/public/favicon.ico       Browser tab icon
  - crates/neuronprompter-web/frontend/public/favicon-32x32.png Browser tab icon (PNG)

Run from the repository root:
    python tools/gen/generate_icon.py

The output files are committed to the repository. The .ico is embedded into
the Windows executable at link time via winres. The .icns is copied into the
macOS .app bundle's Resources directory by the release workflow. The 256x256
PNG is embedded into the Rust binary via include_bytes! for the tao window icon.
"""

import io
import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# --- Brand colors from the Dark Neon Tech design system ---

COLOR_PURPLE = (168, 85, 247)   # #a855f7 -- gradient start
COLOR_CYAN = (34, 211, 238)     # #22d3ee -- gradient end

# Render resolution for the master image. Supersampled at this resolution
# and downscaled to target sizes with LANCZOS resampling for clean edges.
RENDER_SIZE = 2048

# Target icon sizes for multi-resolution .ico and individual PNGs.
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

# ICNS type codes for PNG-based entries in the Apple Icon Image format.
ICNS_TYPES = [
    (b"ic07", 128),    # 128x128
    (b"ic08", 256),    # 256x256
    (b"ic09", 512),    # 512x512
    (b"ic10", 1024),   # 1024x1024 (512x512@2x)
]

# Heavy/Black weight sans-serif fonts to try, in order of preference.
# Black weight produces thicker strokes that remain legible at small icon sizes.
FONT_CANDIDATES = [
    "ariblk.ttf",        # Arial Black (Windows)
    "Arial Black.ttf",   # Arial Black (macOS)
    "impact.ttf",        # Impact (Windows fallback)
    "arialbd.ttf",       # Arial Bold (Windows fallback)
    "Arial Bold.ttf",    # Arial Bold (macOS fallback)
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",  # Linux
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",  # Linux alt
]

# The P glyph is rendered at this fraction of the canvas size.
# A high value ensures the letter fills most of the canvas for small-size legibility.
FONT_SIZE_FRAC = 0.95


def find_font(size: int) -> ImageFont.FreeTypeFont:
    """Locates the first available bold sans-serif font on the system.

    Args:
        size: Font size in pixels.

    Returns:
        A PIL FreeTypeFont instance.

    Raises:
        RuntimeError: If no suitable font is found.
    """
    for name in FONT_CANDIDATES:
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    raise RuntimeError(
        "No suitable bold sans-serif font found. "
        f"Tried: {', '.join(FONT_CANDIDATES)}"
    )


def build_p_mask(size: int) -> Image.Image:
    """Renders a bold "P" glyph as a grayscale mask at the given pixel resolution.

    The mask is white (255) where the P shape is, and black (0) elsewhere.
    The letter is centered horizontally and vertically on the canvas.

    Args:
        size: Canvas width and height in pixels.

    Returns:
        A grayscale (mode "L") PIL Image.
    """
    font_size = int(size * FONT_SIZE_FRAC)
    font = find_font(font_size)

    # Measure the glyph bounding box to center it on the canvas
    tmp = Image.new("L", (size, size), 0)
    tmp_draw = ImageDraw.Draw(tmp)
    bbox = tmp_draw.textbbox((0, 0), "P", font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]

    # Center the glyph, compensating for the bbox offset from origin
    x = (size - text_w) / 2.0 - bbox[0]
    y = (size - text_h) / 2.0 - bbox[1]

    img = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(img)
    draw.text((x, y), "P", fill=255, font=font)
    return img


def build_gradient(size: int, mask: Image.Image) -> Image.Image:
    """Creates an RGBA gradient centered on the P shape's bounding box.

    The gradient runs at 135 degrees (top-left to bottom-right). The exact
    midpoint of the purple-to-cyan transition falls at the geometric center
    of the P shape's bounding box, matching the NeuronCite icon style.

    Args:
        size: Canvas width and height in pixels.
        mask: Grayscale mask defining the P shape's extent.

    Returns:
        An RGBA PIL Image with the gradient applied.
    """
    bbox = mask.getbbox()
    if bbox is None:
        bbox = (0, 0, size, size)

    bb_left, bb_top, bb_right, bb_bottom = bbox
    bb_width = bb_right - bb_left
    bb_height = bb_bottom - bb_top

    gradient = Image.new("RGBA", (size, size))
    pixels = gradient.load()

    for y in range(size):
        ny = (y - bb_top) / bb_height if bb_height > 0 else 0.5
        for x in range(size):
            nx = (x - bb_left) / bb_width if bb_width > 0 else 0.5

            # 135-degree diagonal projection
            diag = nx * 0.5 + ny * 0.5
            t = max(0.0, min(1.0, diag))

            r = int(COLOR_PURPLE[0] + (COLOR_CYAN[0] - COLOR_PURPLE[0]) * t)
            g = int(COLOR_PURPLE[1] + (COLOR_CYAN[1] - COLOR_PURPLE[1]) * t)
            b = int(COLOR_PURPLE[2] + (COLOR_CYAN[2] - COLOR_PURPLE[2]) * t)
            pixels[x, y] = (r, g, b, 255)

    return gradient


def build_icns(sized_images: dict[int, Image.Image]) -> bytes:
    """Constructs an ICNS file (Apple Icon Image format) from pre-rendered images.

    The ICNS binary format consists of a file header followed by a sequence of
    icon entries. Each entry has an 8-byte header (4-byte type code + 4-byte
    entry size) followed by the raw PNG data as payload.

    Args:
        sized_images: Mapping of pixel size to RGBA PIL Image.

    Returns:
        The raw ICNS file bytes.
    """
    entries = b""
    for type_code, size in ICNS_TYPES:
        buf = io.BytesIO()
        sized_images[size].save(buf, format="PNG", optimize=True)
        png_data = buf.getvalue()
        entry_size = 8 + len(png_data)
        entries += type_code + struct.pack(">I", entry_size) + png_data

    total_size = 8 + len(entries)
    return b"icns" + struct.pack(">I", total_size) + entries


def main():
    """Entry point. Renders the master icon and saves all output formats."""
    repo_root = Path(__file__).resolve().parent.parent.parent

    # Output paths
    ico_path = repo_root / "crates" / "neuronprompter" / "assets" / "icon.ico"
    icns_path = repo_root / "crates" / "neuronprompter" / "assets" / "icon.icns"
    png_256_path = repo_root / "crates" / "neuronprompter-web" / "assets" / "icon_256.png"
    favicon_ico_path = repo_root / "crates" / "neuronprompter-web" / "frontend" / "public" / "favicon.ico"
    favicon_png_path = repo_root / "crates" / "neuronprompter-web" / "frontend" / "public" / "favicon-32x32.png"

    # Ensure output directories exist
    for path in [ico_path, icns_path, png_256_path, favicon_ico_path, favicon_png_path]:
        path.parent.mkdir(parents=True, exist_ok=True)

    print(f"Rendering master icon at {RENDER_SIZE}x{RENDER_SIZE}...")

    # Pre-render the high-resolution master and cache it for reuse
    master_mask = build_p_mask(RENDER_SIZE)
    master_gradient = build_gradient(RENDER_SIZE, master_mask)
    master_rgba = Image.new("RGBA", (RENDER_SIZE, RENDER_SIZE), (0, 0, 0, 0))
    master_rgba.paste(master_gradient, mask=master_mask)

    # Collect all required sizes
    icns_sizes = [size for _, size in ICNS_TYPES]
    all_sizes = sorted(set(ICO_SIZES + icns_sizes + [256, 32]))

    # Generate all target sizes by downscaling the master
    sized_images = {}
    for target_size in all_sizes:
        print(f"  Downscaling to {target_size}x{target_size}...")
        sized_images[target_size] = master_rgba.resize(
            (target_size, target_size), Image.LANCZOS
        )

    # --- Save 256x256 PNG for tao window icon ---
    sized_images[256].save(str(png_256_path), "PNG", optimize=True)
    print(f"Window icon PNG saved to: {png_256_path}")

    # --- Save 32x32 PNG favicon ---
    sized_images[32].save(str(favicon_png_path), "PNG", optimize=True)
    print(f"Favicon PNG saved to: {favicon_png_path}")

    # --- Save multi-resolution .ico for Windows executable ---
    ico_base = sized_images[ICO_SIZES[-1]]  # 256px -- largest entry
    ico_rest = [sized_images[s] for s in ICO_SIZES[:-1]]  # 16..128px
    ico_base.save(
        str(ico_path),
        format="ICO",
        append_images=ico_rest,
    )
    print(f"Windows .ico saved to: {ico_path}")

    # --- Save macOS .icns for the .app bundle ---
    icns_data = build_icns(sized_images)
    icns_path.write_bytes(icns_data)
    print(f"macOS .icns saved to: {icns_path}")

    # --- Save browser favicon.ico (16, 32, 48) ---
    favicon_sizes = [16, 32, 48]
    favicon_base = sized_images[favicon_sizes[-1]]  # 48px
    favicon_rest = [sized_images[s] for s in favicon_sizes[:-1]]  # 16, 32
    favicon_base.save(
        str(favicon_ico_path),
        format="ICO",
        append_images=favicon_rest,
    )
    print(f"Browser favicon.ico saved to: {favicon_ico_path}")

    print("Icon generation complete.")


if __name__ == "__main__":
    main()
