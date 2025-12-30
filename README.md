# Bento

A fast texture packer for Godot 4.x with automatic sprite trimming and multiple packing heuristics.

## Features

- **MaxRects bin packing** with multiple heuristics for optimal atlas layout
- **Automatic sprite trimming** removes transparent borders to save atlas space
- **Godot 4.x integration** generates `.tres` AtlasTexture resources with margin support
- **JSON output** for use with other engines or tools
- **Edge extrusion** prevents texture bleeding at sprite boundaries
- **Power-of-two** option for GPU compatibility
- **Multi-atlas support** automatically splits sprites across multiple atlases when needed

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

## Usage

Basic usage:

```bash
bento sprites/ -o output/
```

This packs all images in `sprites/` into atlas(es) in `output/`, generating both Godot `.tres` files and a JSON manifest.

### Examples

Pack sprites with 2px padding and power-of-two dimensions:

```bash
bento sprites/ -o output/ --padding 2 --pot
```

Use the best packing strategy (tries all heuristics and orderings):

```bash
bento sprites/ -o output/ --heuristic best --pack-mode best
```

Add 1px edge extrusion to prevent texture bleeding:

```bash
bento sprites/ -o output/ --extrude 1
```

Output only Godot resources (no JSON):

```bash
bento sprites/ -o output/ --format godot
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-o, --output` | `.` | Output directory |
| `-n, --name` | `atlas` | Base name for output files |
| `--max-width` | `4096` | Maximum atlas width |
| `--max-height` | `4096` | Maximum atlas height |
| `-p, --padding` | `1` | Padding between sprites |
| `-f, --format` | `both` | Output format: `godot`, `json`, or `both` |
| `--no-trim` | off | Disable transparent border trimming |
| `--heuristic` | `best-short-side-fit` | Packing heuristic (see below) |
| `--pack-mode` | `single` | Ordering mode: `single` or `best` |
| `--pot` | off | Force power-of-two dimensions |
| `--extrude` | `0` | Extrude sprite edges by N pixels |
| `--opaque` | off | Output RGB instead of RGBA |
| `-v, --verbose` | off | Verbose output |

### Packing Heuristics

| Heuristic | Description |
|-----------|-------------|
| `best-short-side-fit` | Minimizes the shorter leftover side (default) |
| `best-long-side-fit` | Minimizes the longer leftover side |
| `best-area-fit` | Picks the smallest free rectangle that fits |
| `bottom-left` | Tetris-style packing from bottom-left |
| `contact-point` | Maximizes contact with placed rectangles and edges |
| `best` | Tries all heuristics and picks the most efficient |

### Pack Modes

| Mode | Description |
|------|-------------|
| `single` | Pack sprites in input order (fast) |
| `best` | Try multiple orderings (by area, perimeter, max dimension) and pick the best |

Combine `--heuristic best --pack-mode best` for maximum packing efficiency at the cost of longer processing time.

## Output Formats

### Godot (.tres)

Generates individual `.tres` files for each sprite as AtlasTexture resources:

```gdresource
[gd_resource type="AtlasTexture" load_steps=2 format=3]

[ext_resource type="Texture2D" path="res://atlas_0.png" id="1"]

[resource]
atlas = ExtResource("1")
region = Rect2(0, 0, 64, 64)
margin = Rect2(2, 2, 4, 4)
```

The `margin` field preserves original sprite dimensions when trimming is enabled, so sprite positioning in Godot matches the original artwork.

### JSON

Generates a single JSON manifest compatible with common texture packer formats:

```json
{
  "meta": {
    "app": "bento",
    "version": "0.1.0",
    "format": "rgba8888"
  },
  "atlases": [
    {
      "image": "atlas_0.png",
      "size": { "w": 512, "h": 256 },
      "sprites": [
        {
          "name": "player_idle",
          "frame": { "x": 0, "y": 0, "w": 60, "h": 64 },
          "trimmed": true,
          "spriteSourceSize": { "x": 2, "y": 0, "w": 60, "h": 64 },
          "sourceSize": { "w": 64, "h": 64 }
        }
      ]
    }
  ]
}
```

## License

MIT
