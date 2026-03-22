#!/usr/bin/env python3
"""
Generate the Yap Windows application icon (yap.ico).

Reproduces the macOS AppIcon design: a bold stylized "Y" with audio waveform
bars on each side, rendered on a purple-to-blue gradient rounded-rectangle
background. Outputs a multi-resolution .ico file.
"""

import struct
import math
import io
import os

try:
    from PIL import Image, ImageDraw, ImageFont, ImageFilter
    HAS_PILLOW = True
except ImportError:
    HAS_PILLOW = False


def draw_rounded_rect(draw, xy, radius, fill):
    """Draw a rounded rectangle."""
    x0, y0, x1, y1 = xy
    # Main body
    draw.rectangle([x0 + radius, y0, x1 - radius, y1], fill=fill)
    draw.rectangle([x0, y0 + radius, x1, y1 - radius], fill=fill)
    # Corners
    draw.pieslice([x0, y0, x0 + 2 * radius, y0 + 2 * radius], 180, 270, fill=fill)
    draw.pieslice([x1 - 2 * radius, y0, x1, y0 + 2 * radius], 270, 360, fill=fill)
    draw.pieslice([x0, y1 - 2 * radius, x0 + 2 * radius, y1], 90, 180, fill=fill)
    draw.pieslice([x1 - 2 * radius, y1 - 2 * radius, x1, y1], 0, 90, fill=fill)


def lerp_color(c1, c2, t):
    """Linearly interpolate between two RGB(A) colors."""
    return tuple(int(a + (b - a) * t) for a, b in zip(c1, c2))


def create_gradient_background(size):
    """Create a diagonal gradient from purple to blue with rounded corners."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Purple to blue gradient (matching lava lamp colors)
    c_top_left = (128, 40, 200)      # Purple
    c_bottom_right = (30, 100, 220)  # Blue

    # Draw gradient by horizontal lines
    for y in range(size):
        t = y / max(size - 1, 1)
        color = lerp_color(c_top_left, c_bottom_right, t)
        draw.line([(0, y), (size - 1, y)], fill=(*color, 255))

    # Create rounded mask
    corner_radius = int(size * 0.22)
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    draw_rounded_rect(mask_draw, (0, 0, size - 1, size - 1), corner_radius, fill=255)

    img.putalpha(mask)
    return img


def draw_waveform_bars(draw, cx, cy, size, side, alpha=180):
    """
    Draw audio waveform bars on one side of the Y.
    side: -1 for left, +1 for right
    """
    bar_width = max(2, int(size * 0.028))
    bar_gap = max(1, int(size * 0.018))
    num_bars = 6
    # Heights as a ratio of icon size (symmetrical pattern fading outward)
    heights = [0.28, 0.22, 0.35, 0.18, 0.30, 0.14]

    start_x = cx + side * int(size * 0.18)

    for i in range(num_bars):
        x = start_x + side * i * (bar_width + bar_gap)
        bar_h = int(size * heights[i % len(heights)])
        y0 = cy - bar_h // 2
        y1 = cy + bar_h // 2
        r = bar_width // 2

        # Draw rounded bar (pill shape)
        color = (255, 255, 255, alpha)
        draw.ellipse([x - r, y0 - r, x + r, y0 + r], fill=color)
        draw.ellipse([x - r, y1 - r, x + r, y1 + r], fill=color)
        draw.rectangle([x - r, y0, x + r, y1], fill=color)


def draw_y_letter(draw, cx, cy, size):
    """
    Draw a bold stylized Y letter.
    Uses thick lines to create the Y shape with rounded endpoints.
    """
    stroke_w = max(3, int(size * 0.09))
    half = stroke_w // 2

    # Y dimensions
    top_spread = int(size * 0.20)  # How far the arms spread
    arm_top_y = cy - int(size * 0.28)  # Top of arms
    junction_y = cy + int(size * 0.02)  # Where arms meet
    stem_bottom_y = cy + int(size * 0.28)  # Bottom of stem

    # Draw using thick lines
    # Left arm
    _draw_thick_line(draw, cx - top_spread, arm_top_y, cx, junction_y, stroke_w, (0, 0, 0, 255))
    # Right arm
    _draw_thick_line(draw, cx + top_spread, arm_top_y, cx, junction_y, stroke_w, (0, 0, 0, 255))
    # Stem
    _draw_thick_line(draw, cx, junction_y, cx, stem_bottom_y, stroke_w, (0, 0, 0, 255))

    # Rounded endpoints (circles at each end)
    for px, py in [
        (cx - top_spread, arm_top_y),
        (cx + top_spread, arm_top_y),
        (cx, stem_bottom_y),
    ]:
        draw.ellipse([px - half, py - half, px + half, py + half], fill=(0, 0, 0, 255))


def _draw_thick_line(draw, x0, y0, x1, y1, width, fill):
    """Draw a thick line by drawing a polygon with rounded ends."""
    # Calculate perpendicular offset
    dx = x1 - x0
    dy = y1 - y0
    length = math.sqrt(dx * dx + dy * dy)
    if length == 0:
        return
    nx = -dy / length * width / 2
    ny = dx / length * width / 2

    # Draw polygon for the line body
    points = [
        (x0 + nx, y0 + ny),
        (x0 - nx, y0 - ny),
        (x1 - nx, y1 - ny),
        (x1 + nx, y1 + ny),
    ]
    draw.polygon(points, fill=fill)

    # Draw circles at endpoints for rounded caps
    r = width // 2
    draw.ellipse([x0 - r, y0 - r, x0 + r, y0 + r], fill=fill)
    draw.ellipse([x1 - r, y1 - r, x1 + r, y1 + r], fill=fill)


def draw_white_y_letter(draw, cx, cy, size):
    """
    Draw a bold stylized white Y letter for the icon.
    """
    stroke_w = max(3, int(size * 0.10))
    half = stroke_w // 2
    color = (255, 255, 255, 255)

    top_spread = int(size * 0.18)
    arm_top_y = cy - int(size * 0.25)
    junction_y = cy + int(size * 0.04)
    stem_bottom_y = cy + int(size * 0.28)

    # Left arm
    _draw_thick_line(draw, cx - top_spread, arm_top_y, cx, junction_y, stroke_w, color)
    # Right arm
    _draw_thick_line(draw, cx + top_spread, arm_top_y, cx, junction_y, stroke_w, color)
    # Stem
    _draw_thick_line(draw, cx, junction_y, cx, stem_bottom_y, stroke_w, color)

    # Rounded endpoints
    for px, py in [
        (cx - top_spread, arm_top_y),
        (cx + top_spread, arm_top_y),
        (cx, stem_bottom_y),
    ]:
        draw.ellipse([px - half, py - half, px + half, py + half], fill=color)


def create_icon_pillow(size):
    """Create a single icon frame at the given size using Pillow."""
    # Create gradient background
    # Work at 4x for antialiasing, then downscale
    work_size = max(size * 4, 512)
    img = create_gradient_background(work_size)
    draw = ImageDraw.Draw(img, "RGBA")

    cx = work_size // 2
    cy = work_size // 2

    # Draw waveform bars behind the Y (white, semi-transparent)
    draw_waveform_bars(draw, cx, cy, work_size, -1, alpha=160)
    draw_waveform_bars(draw, cx, cy, work_size, +1, alpha=160)

    # Draw the Y letter in white
    draw_white_y_letter(draw, cx, cy, work_size)

    # Downscale with high-quality resampling
    if work_size != size:
        img = img.resize((size, size), Image.LANCZOS)

    return img


def generate_ico_pillow():
    """Generate yap.ico using Pillow."""
    sizes = [256, 128, 64, 48, 32, 16]
    images = []

    for s in sizes:
        img = create_icon_pillow(s)
        images.append(img)
        print(f"  Generated {s}x{s} frame")

    # Save as ICO with all sizes
    script_dir = os.path.dirname(os.path.abspath(__file__))
    ico_path = os.path.join(script_dir, "yap.ico")
    images[0].save(
        ico_path,
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=images[1:],
    )
    print(f"\nSaved: {ico_path}")
    return ico_path


# ---------------------------------------------------------------------------
# Fallback: raw ICO generation without Pillow
# ---------------------------------------------------------------------------

def create_bmp_data(size):
    """
    Create a simple BMP image (BGRA pixel data) for the icon at the given size.
    Returns raw pixel bytes in bottom-up BGRA order (standard BMP/ICO format).
    """
    pixels = []
    corner_r = int(size * 0.22)

    for row in range(size):
        # BMP rows are bottom-up
        y = size - 1 - row
        for x in range(size):
            # Check if inside rounded rectangle
            inside = True
            # Check corners
            if x < corner_r and y < corner_r:
                if (x - corner_r) ** 2 + (y - corner_r) ** 2 > corner_r ** 2:
                    inside = False
            elif x >= size - corner_r and y < corner_r:
                if (x - (size - 1 - corner_r)) ** 2 + (y - corner_r) ** 2 > corner_r ** 2:
                    inside = False
            elif x < corner_r and y >= size - corner_r:
                if (x - corner_r) ** 2 + (y - (size - 1 - corner_r)) ** 2 > corner_r ** 2:
                    inside = False
            elif x >= size - corner_r and y >= size - corner_r:
                if (x - (size - 1 - corner_r)) ** 2 + (y - (size - 1 - corner_r)) ** 2 > corner_r ** 2:
                    inside = False

            if not inside:
                pixels.extend([0, 0, 0, 0])  # Transparent
                continue

            # Gradient: purple to blue (top to bottom)
            t = y / max(size - 1, 1)
            r = int(128 + (30 - 128) * t)
            g = int(40 + (100 - 40) * t)
            b = int(200 + (220 - 200) * t)

            # Check if this pixel is part of the Y letter (white)
            cx, cy_center = size / 2, size / 2
            # Normalize coordinates
            nx = (x - cx) / size
            ny = (y - cy_center) / size

            is_y = False
            stroke = 0.045

            # Y geometry (normalized)
            arm_top = -0.25
            junction = 0.04
            stem_bottom = 0.28
            spread = 0.18

            # Left arm: from (-spread, arm_top) to (0, junction)
            if arm_top <= ny <= junction:
                t_arm = (ny - arm_top) / (junction - arm_top)
                expected_x = -spread * (1 - t_arm)
                if abs(nx - expected_x) < stroke:
                    is_y = True

            # Right arm: from (spread, arm_top) to (0, junction)
            if arm_top <= ny <= junction:
                t_arm = (ny - arm_top) / (junction - arm_top)
                expected_x = spread * (1 - t_arm)
                if abs(nx - expected_x) < stroke:
                    is_y = True

            # Stem: from (0, junction) to (0, stem_bottom)
            if junction <= ny <= stem_bottom:
                if abs(nx) < stroke:
                    is_y = True

            # Endpoints (circles)
            for px_n, py_n in [(-spread, arm_top), (spread, arm_top), (0, stem_bottom)]:
                if (nx - px_n) ** 2 + (ny - py_n) ** 2 < stroke ** 2:
                    is_y = True

            if is_y:
                r, g, b = 255, 255, 255

            # Check waveform bars
            if not is_y:
                bar_w_n = 0.028
                bar_gap_n = 0.018
                bar_heights = [0.28, 0.22, 0.35, 0.18, 0.30, 0.14]
                for side in [-1, 1]:
                    start_x_n = side * 0.18
                    for i in range(6):
                        bx = start_x_n + side * i * (bar_w_n + bar_gap_n)
                        bh = bar_heights[i % len(bar_heights)] / 2
                        if abs(nx - bx) < bar_w_n / 2 and abs(ny) < bh:
                            r, g, b = 255, 255, 255
                            break

            pixels.extend([b, g, r, 255])  # BGRA

    return bytes(pixels)


def create_ico_entry(size, pixel_data):
    """Create an ICO directory entry and BMP data for one size."""
    # BMP info header (BITMAPINFOHEADER, 40 bytes)
    # Height is 2x because ICO format includes AND mask
    bmp_header = struct.pack(
        "<IiiHHIIiiII",
        40,             # Header size
        size,           # Width
        size * 2,       # Height (2x for ICO)
        1,              # Color planes
        32,             # Bits per pixel
        0,              # Compression (none)
        len(pixel_data),  # Image data size
        0, 0,           # Resolution
        0, 0,           # Colors
    )

    # AND mask: all zeros (fully opaque, alpha channel handles transparency)
    and_mask_row_bytes = ((size + 31) // 32) * 4
    and_mask = b"\x00" * (and_mask_row_bytes * size)

    return bmp_header + pixel_data + and_mask


def generate_ico_raw():
    """Generate yap.ico using raw byte manipulation (no Pillow needed)."""
    sizes = [16, 32, 48, 64, 128, 256]
    entries = []

    for s in sizes:
        print(f"  Generating {s}x{s} frame (raw)...")
        pixel_data = create_bmp_data(s)
        entry_data = create_ico_entry(s, pixel_data)
        entries.append((s, entry_data))

    # ICO file header
    num_images = len(entries)
    header = struct.pack("<HHH", 0, 1, num_images)  # Reserved, type=ICO, count

    # Calculate offsets
    dir_entry_size = 16
    data_offset = 6 + num_images * dir_entry_size  # Header(6) + directory entries

    ico_data = bytearray(header)

    # Build directory entries
    current_offset = data_offset
    for s, entry_data in entries:
        w = 0 if s >= 256 else s  # 0 means 256
        h = 0 if s >= 256 else s
        dir_entry = struct.pack(
            "<BBBBHHII",
            w,                  # Width
            h,                  # Height
            0,                  # Color palette
            0,                  # Reserved
            1,                  # Color planes
            32,                 # Bits per pixel
            len(entry_data),    # Data size
            current_offset,     # Data offset
        )
        ico_data.extend(dir_entry)
        current_offset += len(entry_data)

    # Append image data
    for _, entry_data in entries:
        ico_data.extend(entry_data)

    script_dir = os.path.dirname(os.path.abspath(__file__))
    ico_path = os.path.join(script_dir, "yap.ico")
    with open(ico_path, "wb") as f:
        f.write(ico_data)

    print(f"\nSaved: {ico_path}")
    return ico_path


if __name__ == "__main__":
    print("Generating Yap icon...")
    if HAS_PILLOW:
        print("Using Pillow for high-quality rendering\n")
        path = generate_ico_pillow()
    else:
        print("Pillow not available, using raw ICO generation\n")
        path = generate_ico_raw()

    file_size = os.path.getsize(path)
    print(f"File size: {file_size:,} bytes")
