# GRLE (Giants Run-Length Encoded) File Format Specification

## Complete Reverse-Engineering Documentation for Farming Simulator 25

This document provides a comprehensive technical specification of the GRLE binary file format used in Farming Simulator 25. The format was reverse-engineered through binary analysis and verification against official tool output.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [High-Level Architecture](#2-high-level-architecture)
3. [File Structure](#3-file-structure)
4. [Header Format](#4-header-format)
5. [RLE Compression Algorithm](#5-rle-compression-algorithm)
6. [The Complete Decoding Algorithm](#6-the-complete-decoding-algorithm)
7. [PNG Output Format](#7-png-output-format)
8. [Real-World File Analysis](#8-real-world-file-analysis)
9. [Compression Analysis](#9-compression-analysis)
10. [Implementation Guide](#10-implementation-guide)
11. [Appendix: Hex Dumps](#appendix-hex-dumps)

---

## 1. Introduction

### What are GRLE Files?

GRLE (Giants Run-Length Encoded) files store 2D grayscale bitmap data for Farming Simulator terrain info layers. Unlike GDM files which store density maps for foliage and terrain, GRLE files contain **metadata layers** about the terrain:

- **Environment**: Environmental zones (water, forest, etc.)
- **Farmlands**: Farmland ownership boundaries
- **Field Type**: Field classifications
- **Indoor Mask**: Interior/exterior masks for buildings
- **Collision Layers**: Navigation, placement, and tip collision masks
- **Level Layers**: Lime, plow, spray, roller, stubble shred levels
- **Weed Info**: Weed coverage tracking
- **Soil Map**: Soil type information

### What is Run-Length Encoding (RLE)?

Run-Length Encoding is one of the simplest and oldest compression algorithms, dating back to the 1960s. The core idea is beautifully simple:

**Instead of storing every value individually, store each unique value along with how many times it repeats.**

#### Basic RLE Concept

```
Uncompressed data (15 bytes):
┌───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┐
│ A │ A │ A │ A │ A │ B │ B │ B │ C │ C │ C │ C │ C │ C │ C │
└───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘

RLE compressed (6 bytes):
┌─────────┬─────────┬─────────┐
│ 5 × 'A' │ 3 × 'B' │ 7 × 'C' │
└─────────┴─────────┴─────────┘
   (5,A)     (3,B)     (7,C)
```

#### Why RLE Works Well for Images

RLE excels when data has **long runs of identical values**. This is common in:

- **Bitmap images**: Large areas of the same color
- **Terrain data**: Uniform regions (water, empty land, fields)
- **Masks**: Binary data (0 or 255) with large contiguous areas

```
Example: A simple 8×8 terrain mask

┌───────────────────────────┐
│ 0 0 0 0 0 0 0 0 │  Row 0: 8 zeros
│ 0 0 0 0 0 0 0 0 │  Row 1: 8 zeros
│ 0 0 ■ ■ ■ ■ 0 0 │  Row 2: 2 zeros, 4 ones, 2 zeros
│ 0 0 ■ ■ ■ ■ 0 0 │  Row 3: 2 zeros, 4 ones, 2 zeros
│ 0 0 ■ ■ ■ ■ 0 0 │  Row 4: 2 zeros, 4 ones, 2 zeros
│ 0 0 0 0 0 0 0 0 │  Row 5: 8 zeros
│ 0 0 0 0 0 0 0 0 │  Row 6: 8 zeros
│ 0 0 0 0 0 0 0 0 │  Row 7: 8 zeros
└───────────────────────────┘

Uncompressed: 64 bytes
RLE encoded:  ~20 bytes (varies by implementation)
```

#### When RLE Performs Poorly

RLE can actually **increase** file size for data without repetition:

```
Worst case - alternating values:
┌───┬───┬───┬───┬───┬───┬───┬───┐
│ A │ B │ A │ B │ A │ B │ A │ B │  Original: 8 bytes
└───┴───┴───┴───┴───┴───┴───┴───┘

RLE "compressed":
(1,A)(1,B)(1,A)(1,B)(1,A)(1,B)(1,A)(1,B)  = 16 bytes!
```

This is why RLE is perfect for GRLE files (terrain info layers with large uniform regions) but not suitable for GDM files (which use chunk-based palette compression instead).

### GRLE vs GDM

| Feature | GRLE | GDM |
|---------|------|-----|
| Primary Use | Info layers, collision masks | Density maps (foliage, terrain) |
| Channels | Always 1 (grayscale) | 3-12 channels |
| Compression | Simple RLE | Chunk-based palette |
| Output Format | Always Grayscale PNG | Grayscale or RGB PNG |
| Magic | `GRLE` | `!MDF` or `"MDF` |

### Why Reverse-Engineer This?

GIANTS provides `grleConverter.exe` for Windows, but:

1. No native macOS/Linux support
2. Closed-source with no documentation
3. Understanding enables custom tooling and mod development

### Verification Methodology

This specification has been verified by:

1. Implementing a decoder in Rust
2. Comparing output against the official tool
3. Achieving **0 pixel differences** across all test files

---

## 2. High-Level Architecture

### Simple Linear Storage

Unlike GDM files which use chunked storage, GRLE files use a simple linear approach:

```
┌─────────────────────────────────────────────────────────────────┐
│                    GRLE File Structure                          │
│                                                                 │
│  ┌─────────────────┐                                            │
│  │  20-byte Header │  Version, dimensions, compressed size      │
│  └─────────────────┘                                            │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                                                             ││
│  │              RLE-Compressed Pixel Data                      ││
│  │                                                             ││
│  │    Pixels stored left-to-right, top-to-bottom               ││
│  │    Run-length encoded for efficient compression             ││
│  │                                                             ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

### Why RLE Compression?

Info layers typically have:

- Large uniform regions (empty fields, water bodies)
- Sharp boundaries between regions
- Few unique values per layer

RLE compression excels at this pattern, achieving excellent compression ratios:

```
Raw 4096x4096 image:    16,777,216 bytes
Typical GRLE file:         65,000-650,000 bytes
Compression ratio:         25:1 to 250:1
```

---

## 3. File Structure

### Overall Layout

```
┌────────────────────────────────────────────────┐
│ Offset │ Size │ Description                    │
├────────────────────────────────────────────────┤
│ 0      │ 4    │ Magic: "GRLE" (47 52 4C 45)   │
│ 4      │ 2    │ Version (u16 LE, always 1)    │
│ 6      │ 2    │ Width / 256 (u16 LE)          │
│ 8      │ 2    │ Reserved (0x0000)             │
│ 10     │ 2    │ Height / 256 (u16 LE)         │
│ 12     │ 1    │ Reserved (0x00)               │
│ 13     │ 1    │ Channels (always 1)           │
│ 14     │ 2    │ Reserved (0x0000)             │
│ 16     │ 4    │ Compressed data size (u32 LE) │
│ 20     │ var  │ RLE-compressed pixel data     │
└────────────────────────────────────────────────┘
```

### Dimension Encoding

Dimensions are stored as `dimension / 256`:

| Stored Value | Actual Dimension |
|--------------|------------------|
| 2            | 512              |
| 4            | 1024             |
| 8            | 2048             |
| 16           | 4096             |

This encoding limits dimensions to multiples of 256, which is always the case for FS25 maps.

---

## 4. Header Format

### Detailed Field Breakdown

```
Offset 0-3: Magic Signature
┌─────┬─────┬─────┬─────┐
│ 47  │ 52  │ 4C  │ 45  │  ASCII: "GRLE"
│ 'G' │ 'R' │ 'L' │ 'E' │
└─────┴─────┴─────┴─────┘

Offset 4-5: Version
┌─────┬─────┐
│ 01  │ 00  │  u16 LE = 1 (always)
└─────┴─────┘

Offset 6-7: Width / 256
┌─────┬─────┐
│ 10  │ 00  │  u16 LE = 16 → 4096 pixels
└─────┴─────┘

Offset 8-9: Reserved
┌─────┬─────┐
│ 00  │ 00  │  Always zero
└─────┴─────┘

Offset 10-11: Height / 256
┌─────┬─────┐
│ 10  │ 00  │  u16 LE = 16 → 4096 pixels
└─────┴─────┘

Offset 12: Reserved
┌─────┐
│ 00  │  Always zero
└─────┘

Offset 13: Channels
┌─────┐
│ 01  │  Always 1 (grayscale)
└─────┘

Offset 14-15: Reserved
┌─────┬─────┐
│ 00  │ 00  │  Always zero
└─────┴─────┘

Offset 16-19: Compressed Data Size
┌─────┬─────┬─────┬─────┐
│ xx  │ xx  │ xx  │ xx  │  u32 LE (informational, not used for decoding)
└─────┴─────┴─────┴─────┘
```

### Common Header Values

| File Type | Typical Dimension | Width/256 | Header Hex (6-11) |
|-----------|-------------------|-----------|-------------------|
| environment | 512x512 | 2 | `02 00 00 00 02 00` |
| farmlands | 1024x1024 | 4 | `04 00 00 00 04 00` |
| collision | 2048x2048 | 8 | `08 00 00 00 08 00` |
| most others | 4096x4096 | 16 | `10 00 00 00 10 00` |

---

## 5. RLE Compression Algorithm

### Overview

GRLE uses a variant of Run-Length Encoding optimized for terrain data. The algorithm detects repeated values and encodes them efficiently.

### Encoding Rules

The RLE format works with pairs of bytes and uses special handling for runs:

```
Reading stream:
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│  [skip first byte] → read (prev, new) pairs                      │
│                                                                  │
│  If prev == new:                                                 │
│    → This signals a RUN                                          │
│    → Read count bytes (with 0xFF extension)                      │
│    → Output (count + 2) copies of the value                      │
│                                                                  │
│  If prev != new:                                                 │
│    → TRANSITION between values                                   │
│    → Output 1 copy of prev                                       │
│    → Back up 1 byte (new becomes next prev)                      │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Run Count Encoding

Run counts use a variable-length encoding:

```
Single-byte count (0-254):
┌─────┐
│ N   │  Run length = N + 2
└─────┘

Extended count (255+):
┌─────┬─────┬─────┬─────┐
│ FF  │ FF  │ FF  │ N   │  Run length = 255 + 255 + 255 + N + 2
└─────┴─────┴─────┴─────┘
        ↑     ↑     ↑
    Each 0xFF adds 255 to the count
    Final non-0xFF byte is the remainder
```

### Visual Example

```
Compressed data:    00  05  05  10  03  03  05  07  07  02
                    ↑   ↑   ↑   ↑   ↑   ↑   ↑   ↑   ↑   ↑
                  skip  |   |   |   |   |   |   |   |   |
                        prev=05, new=05 (same!)         |
                             count=10+2=12 copies of 05 |
                                    prev=03, new=03     |
                                         count=5+2=7    |
                                                prev=07, new=07
                                                     count=2+2=4

Decoded output: [05, 05, 05, 05, 05, 05, 05, 05, 05, 05, 05, 05,  (12 copies)
                 03, 03, 03, 03, 03, 03, 03,                      (7 copies)
                 07, 07, 07, 07]                                   (4 copies)
```

### Transition Example

```
Compressed data:    00  05  03  08  08  02
                    ↑   ↑   ↑   ↑   ↑   ↑
                  skip  |   |   |   |   |
                        prev=05, new=03 (different!)
                             output [05], backup, new→prev
                                    prev=03, new=08 (different!)
                                         output [03], backup
                                                prev=08, new=08 (same!)
                                                     count=2+2=4

Decoded output: [05, 03, 08, 08, 08, 08]
```

---

## 6. The Complete Decoding Algorithm

### Pseudocode

```
function decode_grle(data, expected_pixels):
    output = []
    i = 21  // Skip header (20 bytes) + 1 padding byte

    while i + 1 < len(data) AND len(output) < expected_pixels:
        prev = data[i]
        new = data[i + 1]
        i += 2

        if prev == new:
            // RUN: same value repeated
            count = 0
            while i < len(data) AND data[i] == 0xFF:
                count += 255
                i += 1

            if i < len(data):
                count += data[i]
                i += 1

            count += 2  // Counts are offset by 2

            // Emit 'count' copies of the value
            for j in 0 to count:
                output.append(new)
        else:
            // TRANSITION: output previous value
            output.append(prev)
            i -= 1  // Back up so 'new' becomes next 'prev'

    // Pad with zeros if needed (shouldn't happen)
    while len(output) < expected_pixels:
        output.append(0)

    return output
```

### Step-by-Step Decoding

```
Example: 512x512 environment map

1. Read header:
   - Bytes 0-3: "GRLE" ✓
   - Bytes 4-5: Version = 1
   - Bytes 6-7: Width/256 = 2 → 512
   - Bytes 10-11: Height/256 = 2 → 512
   - Bytes 13: Channels = 1

2. Calculate expected size:
   - 512 × 512 × 1 = 262,144 pixels

3. Decode RLE starting at byte 20:
   - Skip first byte (padding/flag)
   - Process pairs until 262,144 pixels decoded

4. Output 512×512 grayscale image
```

---

## 7. PNG Output Format

### Always Grayscale

GRLE files always produce **8-bit grayscale** PNG output:

```
PNG Properties:
  Color Type: Grayscale
  Bit Depth: 8
  Channels: 1
  Bytes per pixel: 1
```

### GIANTS Metadata

The official converter adds text chunks to the PNG:

```
tEXt chunk: "Generated by" = "GIANTS"
tEXt chunk: "Author's name" = "GIANTS Software GmbH"
tEXt chunk: "Author's comments" = "GIANTS Engine"
```

### Pixel Value Meanings

Pixel values depend on the layer type:

| Layer Type | Value Range | Interpretation |
|------------|-------------|----------------|
| environment | 0-255 | Zone IDs |
| farmlands | 0-255 | Farmland IDs (0 = no owner) |
| fieldType | 0-15+ | Field type indices |
| indoorMask | 0/255 | 0 = outdoor, 255 = indoor |
| collision | 0/255 | 0 = passable, 255 = blocked |
| levels | 0-3 | State levels (none/low/medium/high) |
| soilMap | 0-15+ | Soil type indices |

---

## 8. Real-World File Analysis

### File: mapUS_infoLayer_environment.grle

```
Header Analysis:
  Magic: GRLE
  Version: 1
  Dimensions: 512x512
  Channels: 1
  Compressed size: 17,036 bytes

Compression Statistics:
  Raw size: 262,144 bytes
  Compressed: 17,036 bytes
  Ratio: 15.4:1

Content Analysis:
  Most pixels: 0 (empty/default)
  Non-zero pixels: 65,571 (25%)
  Unique values: ~10 zone types
```

### File: mapUS_infoLayer_indoorMask.grle

```
Header Analysis:
  Magic: GRLE
  Version: 1
  Dimensions: 4096x4096
  Channels: 1
  Compressed size: 348,036 bytes

Compression Statistics:
  Raw size: 16,777,216 bytes
  Compressed: 348,036 bytes
  Ratio: 48:1

Content Analysis:
  Binary mask (mostly 0 or 255)
  Indoor areas ~2% of map
```

### Dimension Distribution

From the official converter log, GRLE files use these dimensions:

| Dimension | Count | Use Cases |
|-----------|-------|-----------|
| 512x512 | 3 | environment |
| 1024x1024 | 8 | farmlands, soilMap |
| 2048x2048 | 15 | collision masks |
| 4096x4096 | 81 | most info layers |

---

## 9. Compression Analysis

### Compression Efficiency by Layer Type

| Layer Type | Typical Size | Ratio | Why? |
|------------|--------------|-------|------|
| limeLevel | 65,795 | ~255:1 | Mostly empty (value 0) |
| indoorMask | 348,036 | ~48:1 | Sparse indoor areas |
| tipCollision | 220,590 | ~76:1 | Scattered obstacles |
| placementCollision | 638,247 | ~26:1 | More complex patterns |
| farmlands | 50,545 | ~21:1 | Many distinct regions |

### Why RLE Works Well

```
Typical info layer pattern:

┌───────────────────────────────────────────────┐
│ 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 │ ← Large empty regions
│ 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 │   (compress extremely well)
│ 0 0 0 0 ■ ■ ■ ■ ■ ■ ■ 0 0 0 0 0 0 0 0 0 0 0 0 │
│ 0 0 0 ■ ■ ■ ■ ■ ■ ■ ■ ■ 0 0 0 0 0 0 0 0 0 0 0 │ ← Contiguous features
│ 0 0 0 ■ ■ ■ ■ ■ ■ ■ ■ ■ 0 0 0 0 0 0 0 0 0 0 0 │   (compress well)
│ 0 0 0 0 ■ ■ ■ ■ ■ ■ ■ 0 0 0 0 0 0 0 0 0 0 0 0 │
│ 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 │
└───────────────────────────────────────────────┘

Encoded as: "run of 92 zeros, run of 7 ■s, run of 13 zeros, ..."
```

### Comparison with GDM

```
Same conceptual data:

GDM approach (chunk-based):
┌─────┬─────┬─────┬─────┐
│ HDR │ HDR │ HDR │ HDR │  Each chunk has overhead
├─────┼─────┼─────┼─────┤
│ PAL │ PAL │ PAL │ PAL │  Even empty chunks need headers
├─────┼─────┼─────┼─────┤
│ BMP │ BMP │ BMP │ BMP │
└─────┴─────┴─────┴─────┘

GRLE approach (linear RLE):
┌─────────────────────────────────────────────────┐
│ HDR │ RRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR │
└─────────────────────────────────────────────────┘
      └─── Entire empty regions: just a few bytes
```

---

## 10. Implementation Guide

### Rust Implementation

```rust
fn decode_grle_rle(data: &[u8], expected_size: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(expected_size);
    let mut i = 1; // Skip first byte (padding)

    while i + 1 < data.len() && output.len() < expected_size {
        let prev = data[i];
        let new_val = data[i + 1];
        i += 2;

        if prev == new_val {
            // RUN: read extended count
            let mut count = 0usize;
            while i < data.len() && data[i] == 0xff {
                count += 255;
                i += 1;
            }
            if i < data.len() {
                count += data[i] as usize;
                i += 1;
            }
            count += 2; // Counts offset by 2

            // Emit pixels
            let to_emit = count.min(expected_size - output.len());
            output.extend(std::iter::repeat(new_val).take(to_emit));
        } else {
            // TRANSITION: output prev, backup
            output.push(prev);
            i -= 1;
        }
    }

    output.resize(expected_size, 0);
    output
}

fn convert_grle(input: &[u8]) -> Result<Vec<u8>, Error> {
    // Check magic
    if &input[0..4] != b"GRLE" {
        return Err("Invalid magic");
    }

    // Parse header
    let version = u16::from_le_bytes([input[4], input[5]]);
    let width = u16::from_le_bytes([input[6], input[7]]) as usize * 256;
    let height = u16::from_le_bytes([input[10], input[11]]) as usize * 256;
    let channels = input[13] as usize;

    // Decode
    let expected = width * height * channels;
    let compressed = &input[20..];
    let pixels = decode_grle_rle(compressed, expected);

    Ok(pixels)
}
```

### Python Implementation

```python
def decode_grle(data: bytes) -> bytes:
    """Decode GRLE file to raw pixel data."""

    # Check magic
    if data[:4] != b'GRLE':
        raise ValueError("Invalid GRLE magic")

    # Parse header
    import struct
    version = struct.unpack('<H', data[4:6])[0]
    width = struct.unpack('<H', data[6:8])[0] * 256
    height = struct.unpack('<H', data[10:12])[0] * 256
    channels = data[13]

    expected_size = width * height * channels
    compressed = data[20:]

    # Decode RLE
    output = bytearray()
    i = 1  # Skip first byte

    while i + 1 < len(compressed) and len(output) < expected_size:
        prev = compressed[i]
        new = compressed[i + 1]
        i += 2

        if prev == new:
            # RUN
            count = 0
            while i < len(compressed) and compressed[i] == 0xFF:
                count += 255
                i += 1
            if i < len(compressed):
                count += compressed[i]
                i += 1
            count += 2

            output.extend([new] * min(count, expected_size - len(output)))
        else:
            # TRANSITION
            output.append(prev)
            i -= 1

    # Pad if needed
    output.extend([0] * (expected_size - len(output)))

    return bytes(output), width, height
```

### Key Implementation Notes

1. **Skip first data byte**: The compressed data starts with a padding/flag byte that should be skipped

2. **Count offset**: Run counts are encoded with an offset of 2 (so 0x00 means 2 repetitions)

3. **Extended counts**: 0xFF bytes chain to create large counts (255 + 255 + ... + final)

4. **Backup on transition**: When prev != new, back up 1 byte so the new value becomes the next prev

5. **Always pad**: Ensure output is exactly `width * height` bytes

---

## Appendix: Hex Dumps

### mapUS_infoLayer_environment.grle - First 64 Bytes

```
00000000: 47 52 4c 45 01 00 02 00  00 00 02 00 00 01 00 00  |GRLE............|
00000010: 00 8c 42 00 00 00 00 ff  32 08 08 09 00 00 ff f4  |..B.....2.......|
00000020: 08 08 0a 00 00 ff f3 08  08 09 00 00 ff f4 08 08  |................|
00000030: 09 00 00 ff f3 08 08 0a  00 00 ff f3 08 08 0a 00  |................|

Breakdown:
  Bytes 0-3:   Magic "GRLE"
  Bytes 4-5:   Version = 1
  Bytes 6-7:   Width/256 = 2 → 512
  Bytes 8-9:   Reserved = 0
  Bytes 10-11: Height/256 = 2 → 512
  Byte 12:     Reserved = 0
  Byte 13:     Channels = 1
  Bytes 14-15: Reserved = 0
  Bytes 16-19: Compressed size (informational)
  Byte 20:     Padding (0x00)
  Bytes 21+:   RLE data starts
```

### mapUS_infoLayer_farmlands.grle - First 64 Bytes

```
00000000: 47 52 4c 45 01 00 04 00  00 00 04 00 00 01 00 00  |GRLE............|
00000010: 00 71 c5 00 00 ff ff ff  ff ff ff ff ff ff ff ff  |.q..............|
00000020: ff ff ff ff ff ff ff ff  ff ff ff ff ff ff ff ff  |................|
00000030: ff ff ff ff ff ff ff ff  ff ff ff ff ff ff ff ff  |................|

Breakdown:
  Bytes 0-3:   Magic "GRLE"
  Bytes 4-5:   Version = 1
  Bytes 6-7:   Width/256 = 4 → 1024
  Bytes 10-11: Height/256 = 4 → 1024
  Byte 13:     Channels = 1

Data Analysis:
  Many 0xFF bytes = extended run counts
  Large regions of value 0 (empty farmland)
```

### dlc_infoLayer_indoorMask.grle - First 64 Bytes

```
00000000: 47 52 4c 45 01 00 10 00  00 00 10 00 00 01 00 00  |GRLE............|
00000010: 00 19 ee 04 00 01 01 ff  ff ff ff ff ff ff ff ff  |................|
00000020: ff ff ff ff ff ff ff ff  ff ff ff ff ff ff ff ff  |................|
00000030: ff ff ff ff ff ff ff ff  ff ff ff ff ff ff ff ff  |................|

Breakdown:
  Bytes 0-3:   Magic "GRLE"
  Bytes 6-7:   Width/256 = 16 → 4096
  Bytes 10-11: Height/256 = 16 → 4096

Data Analysis:
  Starts with 01 01 FF FF... = large run of value 1
  Binary mask format (0 or non-zero)
```

---

## Summary

The GRLE format is elegantly simple:

1. **Fixed 20-byte header**: Magic, version, dimensions, channels
2. **Linear RLE compression**: Efficient for sparse info layers
3. **Always grayscale output**: Single-channel 8-bit PNG

Key differences from GDM:

| Aspect | GRLE | GDM |
|--------|------|-----|
| Compression | Simple RLE | Chunk + Palette |
| Header | 20 bytes | 8-16 bytes + chunk table |
| Channels | Always 1 | 3-12 |
| Complexity | Low | Medium-High |

This documentation was created through reverse-engineering and verified to produce **bit-perfect output** compared to the official GIANTS converter tool.

---

_Document version: 1.0_
_Created: 2024_
_Format verified against: grleConverter.exe from Farming Simulator 25_
_Test results: 0 pixel differences across all test files_
