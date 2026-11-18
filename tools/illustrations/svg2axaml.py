#!/usr/bin/env python3
"""
tools/illustrations/svg2axaml.py
Converts hand-drawn SVG exercise illustrations into Avalonia DrawingGroup resources.
"""

import argparse
import json
import os
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ALLOWED_SVG_ATTRS = {'xmlns', 'viewBox', 'width', 'height'}
ALLOWED_PATH_ATTRS = {'d', 'fill', 'fill-opacity', 'stroke', 'stroke-width', 'stroke-linecap', 'stroke-linejoin'}
ALLOWED_CIRCLE_ATTRS = {'cx', 'cy', 'r', 'fill', 'fill-opacity', 'stroke', 'stroke-width', 'stroke-linecap', 'stroke-linejoin'}
ALLOWED_ELLIPSE_ATTRS = {'cx', 'cy', 'rx', 'ry', 'fill', 'fill-opacity', 'stroke', 'stroke-width', 'stroke-linecap', 'stroke-linejoin'}

LINECAP_MAP = {
    'round': 'Round',
    'butt': 'Flat',
    'square': 'Square',
}

LINEJOIN_MAP = {
    'round': 'Round',
    'miter': 'Miter',
    'bevel': 'Bevel',
}


def fail(msg: str):
    print(f"svg2axaml error: {msg}", file=sys.stderr)
    sys.exit(1)


def convert_shape(elem: ET.Element, file_path: str, indent: str = "    ") -> str:
    # Tag without XML namespace
    tag = elem.tag.split('}')[-1]

    if tag == 'path':
        allowed = ALLOWED_PATH_ATTRS
    elif tag == 'circle':
        allowed = ALLOWED_CIRCLE_ATTRS
    elif tag == 'ellipse':
        allowed = ALLOWED_ELLIPSE_ATTRS
    else:
        fail(f"Unsupported element <{tag}> in {file_path}")

    for attr in elem.attrib:
        if attr not in allowed:
            fail(f"Unsupported attribute '{attr}' on <{tag}> in {file_path}")

    if len(elem) > 0:
        fail(f"Element <{tag}> in {file_path} contains unexpected child elements")

    # Parse fill
    fill = elem.attrib.get('fill')
    fill_opacity = elem.attrib.get('fill-opacity')
    has_brush = fill is not None and fill != 'none'

    brush_with_opacity = False
    if has_brush and fill_opacity is not None:
        try:
            opacity_val = float(fill_opacity)
            if opacity_val < 1.0:
                brush_with_opacity = True
        except ValueError:
            fail(f"Invalid fill-opacity '{fill_opacity}' in {file_path}")

    # Parse stroke
    stroke = elem.attrib.get('stroke')
    has_stroke = stroke is not None and stroke != 'none'

    pen_lines = []
    if has_stroke:
        width = elem.attrib.get('stroke-width', '1')
        linecap = elem.attrib.get('stroke-linecap')
        linejoin = elem.attrib.get('stroke-linejoin')

        pen_attr_parts = [f'Brush="{stroke}"', f'Thickness="{width}"']
        if linecap is not None:
            if linecap not in LINECAP_MAP:
                fail(f"Unsupported stroke-linecap '{linecap}' in {file_path}")
            pen_attr_parts.append(f'LineCap="{LINECAP_MAP[linecap]}"')
        if linejoin is not None:
            if linejoin not in LINEJOIN_MAP:
                fail(f"Unsupported stroke-linejoin '{linejoin}' in {file_path}")
            pen_attr_parts.append(f'LineJoin="{LINEJOIN_MAP[linejoin]}"')

        pen_lines.append(f'{indent}    <GeometryDrawing.Pen>')
        pen_lines.append(f'{indent}      <Pen {" ".join(pen_attr_parts)} />')
        pen_lines.append(f'{indent}    </GeometryDrawing.Pen>')

    # Determine Geometry representation
    geom_is_attr = False
    geom_elem_lines = []

    if tag == 'path':
        d = elem.attrib.get('d', '')
        if not d:
            fail(f"Empty path 'd' attribute in {file_path}")
        geom_attr = f'Geometry="{d}"'
        geom_is_attr = True
    elif tag == 'circle':
        cx = elem.attrib.get('cx', '0')
        cy = elem.attrib.get('cy', '0')
        r = elem.attrib.get('r', '0')
        geom_elem_lines.append(f'{indent}    <GeometryDrawing.Geometry>')
        geom_elem_lines.append(f'{indent}      <EllipseGeometry Center="{cx},{cy}" RadiusX="{r}" RadiusY="{r}" />')
        geom_elem_lines.append(f'{indent}    </GeometryDrawing.Geometry>')
    elif tag == 'ellipse':
        cx = elem.attrib.get('cx', '0')
        cy = elem.attrib.get('cy', '0')
        rx = elem.attrib.get('rx', '0')
        ry = elem.attrib.get('ry', '0')
        geom_elem_lines.append(f'{indent}    <GeometryDrawing.Geometry>')
        geom_elem_lines.append(f'{indent}      <EllipseGeometry Center="{cx},{cy}" RadiusX="{rx}" RadiusY="{ry}" />')
        geom_elem_lines.append(f'{indent}    </GeometryDrawing.Geometry>')

    # Assemble GeometryDrawing
    needs_children = brush_with_opacity or has_stroke or (not geom_is_attr)

    drawing_attrs = []
    if has_brush and not brush_with_opacity:
        drawing_attrs.append(f'Brush="{fill}"')
    if geom_is_attr:
        drawing_attrs.append(geom_attr)

    attr_str = (" " + " ".join(drawing_attrs)) if drawing_attrs else ""

    if not needs_children:
        return f'{indent}  <GeometryDrawing{attr_str} />\n'

    lines = [f'{indent}  <GeometryDrawing{attr_str}>']
    if brush_with_opacity:
        lines.append(f'{indent}    <GeometryDrawing.Brush>')
        lines.append(f'{indent}      <SolidColorBrush Color="{fill}" Opacity="{fill_opacity}" />')
        lines.append(f'{indent}    </GeometryDrawing.Brush>')
    if pen_lines:
        lines.extend(pen_lines)
    if geom_elem_lines:
        lines.extend(geom_elem_lines)
    lines.append(f'{indent}  </GeometryDrawing>\n')

    return "\n".join(lines)


def convert_svg_to_drawing_group(svg_path: Path, key: str) -> str:
    tree = ET.parse(svg_path)
    root = tree.getroot()

    root_tag = root.tag.split('}')[-1]
    if root_tag != 'svg':
        fail(f"Expected root <svg> element in {svg_path}, found <{root_tag}>")

    for attr in root.attrib:
        if attr not in ALLOWED_SVG_ATTRS:
            fail(f"Unsupported attribute '{attr}' on <svg> in {svg_path}")

    viewbox = root.attrib.get('viewBox')
    if viewbox and viewbox != '0 0 64 64':
        fail(f"Expected viewBox '0 0 64 64', found '{viewbox}' in {svg_path}")

    group_lines = [f'  <DrawingGroup x:Key="{key}">']
    for child in root:
        shape_axaml = convert_shape(child, str(svg_path), indent="  ")
        group_lines.append(shape_axaml.rstrip())
    group_lines.append('  </DrawingGroup>\n')

    return "\n".join(group_lines)


def generate(repo_root: Path):
    manifest_src = repo_root / "assets" / "illustrations" / "ocy-v1f" / "manifest.json"
    svg_dir = repo_root / "assets" / "illustrations" / "ocy-v1f" / "svg"

    if not manifest_src.exists():
        fail(f"Manifest not found: {manifest_src}")

    manifest_raw = manifest_src.read_text(encoding="utf-8")
    try:
        manifest_data = json.loads(manifest_raw)
    except Exception as e:
        fail(f"Invalid JSON in manifest {manifest_src}: {e}")

    exercises = manifest_data.get("exercises")
    if not isinstance(exercises, dict):
        fail("Manifest missing 'exercises' object")

    groups = []
    # Exercises in manifest order or sorted; keep manifest order
    for ex_id, ex_info in exercises.items():
        frames = ex_info.get("frames")
        if not isinstance(frames, int) or frames < 1:
            fail(f"Exercise '{ex_id}' has invalid frames count: {frames}")

        for theme in ["light", "dark"]:
            for f in range(1, frames + 1):
                svg_file = svg_dir / f"{ex_id}-{theme}-f{f}.svg"
                if not svg_file.exists():
                    fail(f"Frame file not found: {svg_file}")
                key = f"Ill.{ex_id}.{theme}.f{f}"
                group_axaml = convert_svg_to_drawing_group(svg_file, key)
                groups.append(group_axaml)

    axaml_content = (
        '<ResourceDictionary xmlns="https://github.com/avaloniaui"\n'
        '                    xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">\n'
        '  <!-- Generated by tools/illustrations/svg2axaml.py. Do not edit by hand. -->\n\n'
        + "\n".join(groups)
        + '</ResourceDictionary>\n'
    )

    return axaml_content, manifest_raw


def main():
    parser = argparse.ArgumentParser(description="Convert mascot SVG frames to Avalonia AXAML resource dictionary")
    parser.add_argument("--check", action="store_true", help="Check that committed files match fresh generation")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent.parent
    gen_dir = repo_root / "src" / "Ocy.Desktop" / "Illustrations" / "Generated"
    axaml_dest = gen_dir / "Illustrations.axaml"
    manifest_dest = gen_dir / "manifest.json"

    axaml_content, manifest_raw = generate(repo_root)

    if args.check:
        if not axaml_dest.exists():
            fail(f"{axaml_dest} does not exist. Run svg2axaml.py to generate it.")
        if not manifest_dest.exists():
            fail(f"{manifest_dest} does not exist. Run svg2axaml.py to generate it.")

        current_axaml = axaml_dest.read_text(encoding="utf-8")
        current_manifest = manifest_dest.read_text(encoding="utf-8")

        if current_axaml != axaml_content:
            fail(f"{axaml_dest} differs from fresh generation. Run svg2axaml.py to regenerate.")
        if current_manifest != manifest_raw:
            fail(f"{manifest_dest} differs from fresh generation. Run svg2axaml.py to regenerate.")

        print("svg2axaml: --check passed. Generated files match fresh generation.")
        sys.exit(0)
    else:
        gen_dir.mkdir(parents=True, exist_ok=True)
        axaml_dest.write_text(axaml_content, encoding="utf-8")
        manifest_dest.write_text(manifest_raw, encoding="utf-8")
        print(f"Generated {axaml_dest} and {manifest_dest}")


if __name__ == "__main__":
    main()
