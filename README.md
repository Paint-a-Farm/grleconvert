# grleconvert

Cross-platform command-line tool for converting GIANTS Engine density map files (GRLE and GDM) to and from PNG images. These file formats are used in Farming Simulator 22 and Farming Simulator 25.

## Features

- **Decode** GRLE and GDM files to PNG
- **Encode** PNG files back to GRLE and GDM (byte-identical to original)
- Auto-detect encoding parameters from map i3d files
- Lossless conversion with pixel-perfect accuracy
- Fast and lightweight with no external dependencies

## Installation

### From source

Requires [Rust](https://rustup.rs/) 1.70 or later.

```bash
cargo install --git https://github.com/paint-a-farm/grleconvert
```

### Pre-built binaries

Download pre-built binaries for Windows, macOS, and Linux from the [Releases](https://github.com/paint-a-farm/grleconvert/releases) page.

## Usage

### Decoding (GRLE/GDM to PNG)

```bash
# Convert GRLE file to PNG
grleconvert input.grle output.png

# Convert GDM file to PNG
grleconvert input.gdm output.png

# If output path is omitted, uses input filename with .png extension
grleconvert map_densityMap_height.gdm
```

### Encoding (PNG to GRLE/GDM)

```bash
# Convert PNG to GRLE/GDM (auto-detected from i3d file in directory hierarchy)
grleconvert infoLayer_farmlands.png

# Convert PNG to GRLE (auto-detected from output extension)
grleconvert input.png output.grle

# Convert PNG to GDM (requires channel info from i3d or manual parameters)
grleconvert input.png output.gdm --channels 3

# With compression split for multi-range GDM files (e.g., height maps)
grleconvert input.png output.gdm --channels 12 --compress-at 8

# Specify i3d file explicitly for parameter discovery
grleconvert input.png output.gdm --i3d /path/to/map.i3d
```

**Parameter discovery:**

When encoding, the tool automatically searches for a map `.i3d` file in the directory hierarchy to determine encoding parameters. If no i3d is found:

- For GRLE output (`.grle` extension): works without additional parameters
- For GDM output: requires `--channels <n>` and optionally `--compress-at <n>`

You can also specify an i3d file explicitly with `--i3d <path>`.

### Additional utilities

```bash
# Compare two PNG files for differences (useful for verifying output against the official tool)
compare_pngs file1.png file2.png
```

## Supported formats

### GRLE (GIANTS Run-Length Encoded)

- Magic: `GRLE`
- Always grayscale (1 channel)
- Used for: infoLayer files (farmlands, field types, collision maps, soil maps, etc.)

**Header format (20 bytes):**

| Offset | Size | Field    | Description                        |
| ------ | ---- | -------- | ---------------------------------- |
| 0      | 4    | Magic    | `GRLE`                             |
| 4      | 2    | Version  | Always 1                           |
| 6      | 2    | Width    | Image width / 256                  |
| 8      | 2    | Padding  | Always 0                           |
| 10     | 2    | Height   | Image height / 256                 |
| 12     | 2    | Unknown  | Always 256                         |
| 14     | 2    | Padding  | Always 0                           |
| 16     | 4    | CompSize | 0x00 + 3-byte LE (data_length - 1) |

**RLE encoding:**

- Initial 0x00 padding byte
- Decoder reads pairs (a, b): if a == b, it's a run with count bytes following; if different, emit a and back up
- Each pixel value appears once in stream, except runs which have value twice followed by count
- Count encoding: 0xff bytes add 255 each, final byte is remainder, total pixels = count + 2

### GDM (GIANTS Density Map)

- Magic: `"MDF` or `!MDF`
- Grayscale (1-8 channels) or RGB (9+ channels)
- Used for: densityMap files (height, ground, foliage, stones, etc.)

See [GDM_FORMAT.md](docs/GDM_FORMAT.md) and [GRLE_FORMAT.md](docs/GRLE_FORMAT.md) for detailed format documentation.

## Building

```bash
# Debug build
cargo build

# Release build (optimized, smaller binary)
cargo build --release
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

Format reverse-engineered from the official GIANTS `grleConverter.exe` tool output.
