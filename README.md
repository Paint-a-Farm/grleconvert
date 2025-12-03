# grleconvert

Cross-platform command-line tool for converting GIANTS Engine density map files (GRLE and GDM) to PNG images. These file formats are used in Farming Simulator 22 and Farming Simulator 25.

## Features

- Convert GRLE (GIANTS Run-Length Encoded) files to grayscale PNG
- Convert GDM (GIANTS Density Map) files to grayscale or RGB PNG
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

```bash
# Convert GRLE file to PNG
grleconvert input.grle output.png

# Convert GDM file to PNG
grleconvert input.gdm output.png

# If output path is omitted, uses input filename with .png extension
grleconvert map_densityMap_height.gdm
```

### Additional utilities

```bash
# Compare two PNG files for differences (useful for verifying output against the official tool)
compare_pngs file1.png file2.png
```

## Supported formats

### GRLE (GIANTS Run-Length Encoded)

- Magic: `GRLE`
- Always grayscale (1 channel)
- Used for: infoLayer files (farmlands, field types, etc.)

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

Format reverse-engineered from the official GIANTS `grleConverter.exe` tool.
