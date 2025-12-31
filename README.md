# Bento

A fast sprite atlas packer with automatic trimming and multiple packing heuristics.

## Features

- **MaxRects bin packing** with multiple heuristics for optimal atlas layout
- **Automatic sprite trimming** removes transparent borders to save atlas space
- **JSON output** (recommended) for efficient loading via a simple autoload script
- **Godot 4.x integration** generates `.tres` AtlasTexture resources with margin support
- **Edge extrusion** prevents texture bleeding at sprite boundaries
- **Power-of-two** option for GPU compatibility
- **Multi-atlas support** automatically splits sprites across multiple atlases when needed
- **PNG compression** with oxipng for smaller file sizes

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

## Usage

Bento uses subcommands to select the output format:

```bash
bento json sprites/*.png -o output/   # JSON metadata (recommended)
bento godot sprites/*.png -o output/  # Individual Godot .tres files
```

### Examples

Pack sprites with JSON output (recommended for Godot):

```bash
bento json sprites/*.png -o output/
```

Pack with 2px padding and power-of-two dimensions:

```bash
bento json sprites/*.png -o output/ --padding 2 --pot
```

Use the best packing strategy (tries all heuristics and orderings):

```bash
bento json sprites/*.png -o output/ --heuristic best --pack-mode best
```

Add 1px edge extrusion to prevent texture bleeding:

```bash
bento json sprites/*.png -o output/ --extrude 1
```

Output individual Godot .tres files:

```bash
bento godot sprites/*.png -o output/
```

Compress PNG output for smaller file sizes:

```bash
bento json sprites/*.png -o output/ --compress        # default level (2)
bento json sprites/*.png -o output/ --compress 6      # higher compression
bento json sprites/*.png -o output/ --compress max    # maximum compression (slower)
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-o, --output` | `.` | Output directory |
| `-n, --name` | `atlas` | Base name for output files |
| `--max-width` | `4096` | Maximum atlas width |
| `--max-height` | `4096` | Maximum atlas height |
| `-p, --padding` | `1` | Padding between sprites |
| `--no-trim` | off | Disable transparent border trimming |
| `--trim-margin` | `0` | Keep N pixels of transparent border after trimming |
| `--heuristic` | `best-short-side-fit` | Packing heuristic (see below) |
| `--pack-mode` | `single` | Ordering mode: `single` or `best` |
| `--pot` | off | Force power-of-two dimensions |
| `--extrude` | `0` | Extrude sprite edges by N pixels |
| `--opaque` | off | Output RGB instead of RGBA |
| `--compress` | off | PNG compression level (0-6 or `max`) |
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

### JSON (Recommended)

Generates a single JSON manifest. This is the recommended format for Godot projects because:
- Single file instead of hundreds of `.tres` files
- Cleaner git diffs when repacking
- Faster loading (one file operation vs many)
- Works with other engines

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

#### Using JSON with Godot

Create an autoload script to load sprites from the JSON:

```gdscript
# autoload/sprites.gd
extends Node

var _atlases: Array[Texture2D] = []
var _sprites: Dictionary = {}
var _cache: Dictionary = {}

func _ready() -> void:
    _load_atlas("res://assets/atlas/atlas.json")

func _load_atlas(json_path: String) -> void:
    var file := FileAccess.open(json_path, FileAccess.READ)
    if not file:
        push_error("Failed to load atlas: " + json_path)
        return

    var data: Dictionary = JSON.parse_string(file.get_as_text())
    var base_path := json_path.get_base_dir()

    for atlas_data in data.atlases:
        var atlas_path := base_path.path_join(atlas_data.image)
        _atlases.append(load(atlas_path))

    for atlas_idx in data.atlases.size():
        for s in data.atlases[atlas_idx].sprites:
            var sss = s.spriteSourceSize
            var src = s.sourceSize
            _sprites[s.name] = {
                "atlas": atlas_idx,
                "frame": s.frame,
                "trimmed": s.trimmed,
                "margin_left": sss.x,
                "margin_top": sss.y,
                "margin_right": src.w - sss.x - sss.w,
                "margin_bottom": src.h - sss.y - sss.h,
            }

func get(sprite_name: String) -> AtlasTexture:
    if sprite_name in _cache:
        return _cache[sprite_name]

    if sprite_name not in _sprites:
        push_error("Sprite not found: " + sprite_name)
        return null

    var data: Dictionary = _sprites[sprite_name]
    var f: Dictionary = data.frame

    var tex := AtlasTexture.new()
    tex.atlas = _atlases[data.atlas]
    tex.region = Rect2(f.x, f.y, f.w, f.h)

    if data.trimmed:
        tex.margin = Rect2(
            data.margin_left, data.margin_top,
            data.margin_right, data.margin_bottom
        )

    tex.filter_clip = true
    _cache[sprite_name] = tex
    return tex
```

Register as autoload in Project Settings, then use:

```gdscript
$Sprite2D.texture = Sprites.get("player_idle")
```

### Godot (.tres)

Generates individual `.tres` files for each sprite as AtlasTexture resources. Use this if you prefer native Godot resources with editor drag-and-drop support.

```gdresource
[gd_resource type="AtlasTexture" load_steps=2 format=3]

[ext_resource type="Texture2D" path="res://atlas_0.png" id="1"]

[resource]
atlas = ExtResource("1")
region = Rect2(0, 0, 64, 64)
margin = Rect2(2, 2, 4, 4)
```

The `margin` field preserves original sprite dimensions when trimming is enabled.

## License

MIT
