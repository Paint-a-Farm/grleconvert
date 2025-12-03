# GDM (Giants Density Map) File Format Specification

## Complete Reverse-Engineering Documentation for Farming Simulator 25

This document provides a comprehensive technical specification of the GDM binary file format used in Farming Simulator 25. The format was reverse-engineered by analyzing the GIANTS `grleConverter.exe` tool using Hex-Rays decompiler, combined with extensive binary analysis and verification against official tool output.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [High-Level Architecture](#2-high-level-architecture)
3. [File Structure Overview](#3-file-structure-overview)
4. [Header Formats](#4-header-formats)
5. [Compression Ranges Deep Dive](#5-compression-ranges-deep-dive)
6. [Block Format Specification](#6-block-format-specification)
7. [Bitmap Encoding Details](#7-bitmap-encoding-details)
8. [The Complete Decoding Algorithm](#8-the-complete-decoding-algorithm)
9. [PNG Output Generation](#9-png-output-generation)
10. [Real-World File Analysis](#10-real-world-file-analysis)
11. [Compression Analysis](#11-compression-analysis)
12. [Implementation Guide](#12-implementation-guide)
13. [Appendix: Hex Dumps](#appendix-hex-dumps)

---

## 1. Introduction

### What are GDM Files?

GDM (Giants Density Map) files store 2D bitmap data for Farming Simulator terrain. They contain information about:

- **Crops/Fruits**: What's planted where, growth stages, spray states
- **Stones**: Rock locations and sizes
- **Weeds**: Weed coverage
- **Ground**: Terrain texture indices
- **Height**: Terrain elevation data
- **Various info layers**: Farmland ownership, field types, collision maps, etc.

### Why Reverse-Engineer This?

GIANTS provides a command-line tool (`grleConverter.exe`) to convert between GDM and PNG formats, but:

1. It's Windows-only (no native macOS/Linux support)
2. It's closed-source with no documentation
3. Understanding the format enables custom tooling and mod development

### Verification Methodology

Every aspect of this specification has been verified by:

1. Implementing a decoder in Rust
2. Comparing output against the official tool
3. Achieving **0 pixel differences** across all test files

---

## 2. High-Level Architecture

### The Chunking Concept

GDM files don't store pixels sequentially. Instead, they use a **tile-based** or **chunk-based** approach:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         4096 × 4096 Pixel Image                             │
│                                                                             │
│  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─── ─ ─ ─ ─┬─────┐       │
│  │     │     │     │     │     │     │     │     │           │     │       │
│  │  0  │  1  │  2  │  3  │  4  │  5  │  6  │  7  │    ...    │ 127 │  Row 0│
│  │     │     │     │     │     │     │     │     │           │     │       │
│  ├─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─── ─ ─ ─ ─┼─────┤       │
│  │     │     │     │     │     │     │     │     │           │     │       │
│  │ 128 │ 129 │ 130 │ 131 │ 132 │ 133 │ 134 │ 135 │    ...    │ 255 │  Row 1│
│  │     │     │     │     │     │     │     │     │           │     │       │
│  ├─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─── ─ ─ ─ ─┼─────┤       │
│  │     │     │     │     │     │     │     │     │           │     │       │
│  │ 256 │ 257 │ 258 │     │     │     │     │     │    ...    │     │  Row 2│
│  │     │     │     │     │     │     │     │     │           │     │       │
│  ├─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─── ─ ─ ─ ─┴─────┤       │
│  │                            ...                                  │       │
│  │                                                                 │       │
│  ├─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─── ─ ─ ─ ─┬─────┤       │
│  │     │     │     │     │     │     │     │     │           │     │       │
│  │16256│16257│     │     │     │     │     │     │    ...    │16383│Row 127│
│  │     │     │     │     │     │     │     │     │           │     │       │
│  └─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─── ─ ─ ─ ─┴─────┘       │
│                                                                             │
│  Each small box = one 32×32 pixel chunk                                     │
│  Total chunks = 128 × 128 = 16,384                                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Why Chunking?

Chunking provides several benefits:

1. **Spatial locality**: Nearby pixels are stored together
2. **Efficient compression**: Each chunk can use optimal encoding
3. **Random access**: Can decode any chunk independently
4. **Uniform regions**: Large areas of same value compress extremely well

### The Channel Concept

Each pixel in a GDM file has multiple **channels** (bit planes). Think of it like this:

```
Traditional RGB Image:          GDM Multi-Channel Data:
┌───────────────────┐          ┌───────────────────────────────┐
│ R │ G │ B │       │          │ Ch0 │ Ch1 │ Ch2 │ ... │ Ch9 │  │
│ 8 │ 8 │ 8 │ bits  │          │  1  │  1  │  1  │     │  1  │ bits
└───────────────────┘          └───────────────────────────────┘
     24 bits total                    10 bits total (example)
```

For a **fruits** density map, these channels might represent:

- Bits 0-4: Growth state (32 possible values)
- Bits 5-7: Crop type index (8 types)
- Bit 8: Sprayed flag
- Bit 9: Fertilized flag

---

## 3. File Structure Overview

### Complete File Layout

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              GDM FILE                                      │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                         HEADER                                       │  │
│  │                      (9 or 16 bytes)                                 │  │
│  │                                                                      │  │
│  │  Contains: Magic, dimension, chunk size, channel count,              │  │
│  │           compression range count, etc.                              │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                   COMPRESSION BOUNDARIES                             │  │
│  │              (num_compression_ranges - 1 bytes)                      │  │
│  │                                                                      │  │
│  │  Defines where channel groups split for separate compression         │  │
│  │  Example: [5] means range0=ch0-4, range1=ch5-9                       │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │              TYPE INDEX MAPPINGS (optional)                          │  │
│  │            (3 × type_index_channels bytes)                           │  │
│  │                                                                      │  │
│  │  Advanced feature for type-indexed channel remapping                 │  │
│  │  Only present in "MDF format when type_index_channels > 0            │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                        DATA BLOCKS                                   │  │
│  │                    (variable size each)                              │  │
│  │                                                                      │  │
│  │  Sequential blocks, one per (chunk × compression_range)              │  │
│  │  Total blocks = total_chunks × num_compression_ranges                │  │
│  │                                                                      │  │
│  │  ┌────────────┐┌────────────┐┌────────────┐┌────────────┐           │  │
│  │  │  Block 0   ││  Block 1   ││  Block 2   ││  Block 3   │  ...      │  │
│  │  │ Chunk 0    ││ Chunk 0    ││ Chunk 1    ││ Chunk 1    │           │  │
│  │  │ Range 0    ││ Range 1    ││ Range 0    ││ Range 1    │           │  │
│  │  └────────────┘└────────────┘└────────────┘└────────────┘           │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

### Data Flow Visualization

```
                    GDM File
                       │
                       ▼
              ┌────────────────┐
              │  Parse Header  │
              └────────┬───────┘
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
    Dimension    Channels    Comp. Ranges
    (4096)         (10)          (2)
         │             │             │
         └─────────────┼─────────────┘
                       │
                       ▼
              ┌────────────────┐
              │ Read Boundaries│  → [0, 5, 10]
              └────────┬───────┘
                       │
                       ▼
              ┌────────────────┐
              │  For Each      │
              │    Chunk       │  16,384 iterations
              │    (0..16383)  │
              └────────┬───────┘
                       │
         ┌─────────────┴─────────────┐
         ▼                           ▼
   ┌───────────┐               ┌───────────┐
   │ Decode    │               │ Decode    │
   │ Block     │               │ Block     │
   │ Range 0   │               │ Range 1   │
   │ (5 bits)  │               │ (5 bits)  │
   └─────┬─────┘               └─────┬─────┘
         │                           │
         └───────────┬───────────────┘
                     │
                     ▼
              ┌────────────────┐
              │    Combine     │
              │                │
              │ val = r0 |     │
              │   (r1 << 5)    │
              └────────┬───────┘
                       │
                       ▼
              ┌────────────────┐
              │  Write Pixel   │
              │    to Image    │
              └────────────────┘
```

---

## 4. Header Formats

### 4.1 !MDF Format (0x21) - The Simple Format

This is the more common format in FS25. It has a compact 9-byte header.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           !MDF HEADER (9 bytes)                             │
├─────────┬──────┬────────────────────────────────────────────────────────────┤
│ Offset  │ Size │ Description                                                │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x00    │  1   │ Magic byte 1: '!' (0x21)                                   │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x01    │  1   │ Magic byte 2: 'M' (0x4D)                                   │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x02    │  1   │ Magic byte 3: 'D' (0x44)                                   │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x03    │  1   │ Magic byte 4: 'F' (0x46)                                   │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x04    │  1   │ dim_log2: Dimension exponent                               │
│         │      │   dimension = 2^(dim_log2 + 5)                             │
│         │      │   Example: 7 → 2^12 = 4096                                 │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x05    │  1   │ chunk_log2: Chunk size exponent                            │
│         │      │   chunk_size = 2^chunk_log2                                │
│         │      │   Always 5 → 32 pixels                                     │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x06    │  1   │ max_bpp: Maximum bits per pixel                            │
│         │      │   Typically 1 or 2                                         │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x07    │  1   │ num_channels: Total bit channels                           │
│         │      │   Range: 1-24 (practical range 3-12)                       │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x08    │  1   │ num_compression_ranges: Number of channel groups           │
│         │      │   Range: 1-6 (typically 1-2)                               │
└─────────┴──────┴────────────────────────────────────────────────────────────┘
```

#### Dimension Calculation

The dimension formula `2^(dim_log2 + 5)` is designed so that:

| dim_log2 | Calculation | Dimension |
| -------- | ----------- | --------- |
| 0        | 2^5         | 32        |
| 1        | 2^6         | 64        |
| 2        | 2^7         | 128       |
| 3        | 2^8         | 256       |
| 4        | 2^9         | 512       |
| 5        | 2^10        | 1024      |
| 6        | 2^11        | 2048      |
| 7        | 2^12        | 4096      |
| 8        | 2^13        | 8192      |

#### Real Example: mapUS_densityMap_fruits.gdm

```
Hex dump of header:
21 4D 44 46 07 05 02 0A 02

Parsing:
  Magic:                    "!MDF"
  dim_log2:                 0x07 = 7
    → dimension = 2^(7+5) = 2^12 = 4096
  chunk_log2:               0x05 = 5
    → chunk_size = 2^5 = 32
  max_bpp:                  0x02 = 2
  num_channels:             0x0A = 10
  num_compression_ranges:   0x02 = 2
```

### 4.2 "MDF Format (0x22) - The Extended Format

This format includes a version field and type index channel support.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          "MDF HEADER (16 bytes)                             │
├─────────┬──────┬────────────────────────────────────────────────────────────┤
│ Offset  │ Size │ Description                                                │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x00    │  4   │ Magic: '"MDF' (0x22, 0x4D, 0x44, 0x46)                     │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x04    │  4   │ Version (little-endian u32)                                │
│         │      │   Must be 0                                                │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x08    │  1   │ dim_log2: Same as !MDF                                     │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x09    │  1   │ chunk_log2: Same as !MDF                                   │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x0A    │  1   │ max_bpp: Same as !MDF                                      │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x0B    │  1   │ num_channels: Same as !MDF                                 │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x0C    │  1   │ num_compression_ranges: Same as !MDF                       │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x0D    │  1   │ type_index_channels: Advanced remapping feature            │
│         │      │   Usually 0                                                │
├─────────┼──────┼────────────────────────────────────────────────────────────┤
│ 0x0E    │  2   │ Reserved/padding (usually 0x0000)                          │
└─────────┴──────┴────────────────────────────────────────────────────────────┘
```

#### Real Example: mapAS_densityMap_stones.gdm

```
Hex dump of header:
22 4D 44 46 00 00 00 00 07 05 02 03 01 00 00 00

Parsing:
  Magic:                    '"MDF'
  Version:                  0x00000000 = 0 ✓
  dim_log2:                 0x07 = 7 → 4096
  chunk_log2:               0x05 = 5 → 32
  max_bpp:                  0x02 = 2
  num_channels:             0x03 = 3
  num_compression_ranges:   0x01 = 1
  type_index_channels:      0x00 = 0
```

---

## 5. Compression Ranges Deep Dive

### What Are Compression Ranges?

Compression ranges allow splitting the channels into groups that are compressed independently. This is beneficial because:

1. **Different data patterns**: Growth states change often, crop types rarely
2. **Better compression**: Similar data compresses better together
3. **Parallel processing**: Ranges can be decoded independently

### Boundary Definition

After the header, if `num_compression_ranges > 1`, there are `num_compression_ranges - 1` boundary bytes.

```
Example: 10 channels, 2 ranges

Header: num_channels=10, num_compression_ranges=2
Next byte after header: 0x05 (boundary)

Interpretation:
  Range 0: channels 0 through 4  (5 channels)
  Range 1: channels 5 through 9  (5 channels)

  Boundaries array: [0, 5, 10]
                     │  │  └── End (= num_channels)
                     │  └───── Boundary byte value
                     └──────── Implicit start
```

### Visual: How Ranges Split the Pixel Value

```
                     10-bit Pixel Value
    ┌────────────────────────────────────────────────┐
    │ b9 │ b8 │ b7 │ b6 │ b5 │ b4 │ b3 │ b2 │ b1 │ b0 │
    └────────────────────────────────────────────────┘
      └─────────────────┘   └─────────────────────────┘
           Range 1                   Range 0
        (5 channels)              (5 channels)
         ch 5-9                     ch 0-4

    In the file:
    ┌──────────────┐ ┌──────────────┐
    │   Block A    │ │   Block B    │
    │   Range 0    │ │   Range 1    │
    │  (5 bits)    │ │  (5 bits)    │
    └──────────────┘ └──────────────┘

    Recombination:
    pixel_value = block_a_value | (block_b_value << 5)
```

### Complex Example: 12 Channels, 2 Ranges

```
mapUS_densityMap_height.gdm:
  num_channels = 12
  num_compression_ranges = 2
  boundary byte = 0x06

  Range 0: channels 0-5  (6 channels, 6 bits)
  Range 1: channels 6-11 (6 channels, 6 bits)

                     12-bit Pixel Value
    ┌─────────────────────────────────────────────────────────┐
    │b11│b10│ b9│ b8│ b7│ b6│ b5│ b4│ b3│ b2│ b1│ b0│
    └─────────────────────────────────────────────────────────┘
      └───────────────────┘   └───────────────────────────────┘
            Range 1                      Range 0
          (6 channels)                 (6 channels)

    pixel_value = range0_value | (range1_value << 6)
```

### Block Ordering with Multiple Ranges

```
For a file with 4 chunks and 2 compression ranges:

Block Index:   0      1      2      3      4      5      6      7
Chunk Index:   0      0      1      1      2      2      3      3
Range Index:   0      1      0      1      0      1      0      1

Visual layout:
┌─────────────────────────────────────────────────────────────────┐
│ Chunk 0, R0 │ Chunk 0, R1 │ Chunk 1, R0 │ Chunk 1, R1 │  ...   │
└─────────────────────────────────────────────────────────────────┘

Reading loop:
for chunk_idx in 0..total_chunks {
    for range_idx in 0..num_compression_ranges {
        read_block();  // Block (chunk_idx * num_ranges + range_idx)
    }
}
```

---

## 6. Block Format Specification

### Block Structure

Each block contains compressed data for one 32×32 pixel chunk and one compression range.

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              BLOCK                                         │
├────────────┬───────────────────────────────────────────────────────────────┤
│   Offset   │                        Content                                │
├────────────┼───────────────────────────────────────────────────────────────┤
│     0      │  bit_depth (1 byte)                                           │
│            │    0 = uniform block (all pixels identical)                   │
│            │    1 = 1 bit per pixel (2 unique values)                      │
│            │    2 = 2 bits per pixel (up to 4 unique values)               │
│            │    3+ = higher bit depths for complex chunks                  │
├────────────┼───────────────────────────────────────────────────────────────┤
│     1      │  palette_count (1 byte)                                       │
│            │    Number of unique values in this chunk                      │
│            │    For bit_depth ≤ 2: used as lookup table                    │
│            │    For bit_depth > 2: may be 0 (raw values)                   │
├────────────┼───────────────────────────────────────────────────────────────┤
│     2      │  palette[0] (2 bytes, little-endian u16)                      │
├────────────┼───────────────────────────────────────────────────────────────┤
│     4      │  palette[1] (2 bytes, if palette_count > 1)                   │
├────────────┼───────────────────────────────────────────────────────────────┤
│    ...     │  palette[2..palette_count-1]                                  │
├────────────┼───────────────────────────────────────────────────────────────┤
│ 2+2*count  │  bitmap data (bit_depth × 128 bytes)                          │
│            │    Not present if bit_depth = 0                               │
└────────────┴───────────────────────────────────────────────────────────────┘
```

### Block Size Formula

```
block_size = 2 + (2 × palette_count) + (bit_depth × 128)

Examples:
  Uniform block (bit_depth=0, palette_count=1):
    2 + 2×1 + 0×128 = 4 bytes

  2-value block (bit_depth=1, palette_count=2):
    2 + 2×2 + 1×128 = 134 bytes

  4-value block (bit_depth=2, palette_count=4):
    2 + 2×4 + 2×128 = 266 bytes

  Raw 5-bit block (bit_depth=5, palette_count=0):
    2 + 2×0 + 5×128 = 642 bytes
```

### Why 128 Bytes Per Bit?

A 32×32 chunk contains 1024 pixels:

```
At 1 bit per pixel:
  1024 pixels × 1 bit = 1024 bits = 128 bytes

At 2 bits per pixel:
  1024 pixels × 2 bits = 2048 bits = 256 bytes = 2 × 128

At N bits per pixel:
  1024 pixels × N bits = N × 1024 bits = N × 128 bytes
```

### Bit Depth Analysis

| bit_depth | Bitmap Size | Max Palette | Bits/Pixel | Use Case                     |
| --------- | ----------- | ----------- | ---------- | ---------------------------- |
| 0         | 0 bytes     | 1           | 0          | Uniform areas (water, empty) |
| 1         | 128 bytes   | 2           | 1          | Binary patterns (yes/no)     |
| 2         | 256 bytes   | 4           | 2          | 4-state data                 |
| 3         | 384 bytes   | 8           | 3          | 8-state data                 |
| 4         | 512 bytes   | 16          | 4          | 16-state data                |
| 5         | 640 bytes   | 32          | 5          | Complex data (usually raw)   |
| 6         | 768 bytes   | 64          | 6          | Height data ranges           |

---

## 7. Bitmap Encoding Details

### Bit Packing Order

Bits are packed LSB-first (least significant bit first) within each byte:

```
For bit_depth = 2:

Byte 0:                    Byte 1:
┌──┬──┬──┬──┬──┬──┬──┬──┐  ┌──┬──┬──┬──┬──┬──┬──┬──┐
│b7│b6│b5│b4│b3│b2│b1│b0│  │b7│b6│b5│b4│b3│b2│b1│b0│
└──┴──┴──┴──┴──┴──┴──┴──┘  └──┴──┴──┴──┴──┴──┴──┴──┘
 └─┬─┘ └─┬─┘ └─┬─┘ └─┬─┘    └─┬─┘ └─┬─┘ └─┬─┘ └─┬─┘
  px3   px2   px1   px0      px7   px6   px5   px4
```

### Extraction Algorithm

```rust
fn extract_value(bitmap: &[u8], pixel_idx: usize, bit_depth: usize) -> u16 {
    // Calculate bit position
    let bit_pos = pixel_idx * bit_depth;
    let byte_idx = bit_pos / 8;
    let bit_offset = bit_pos % 8;

    // Read 2 bytes (handles values spanning byte boundaries)
    let mut raw = bitmap[byte_idx] as u16;
    if byte_idx + 1 < bitmap.len() {
        raw |= (bitmap[byte_idx + 1] as u16) << 8;
    }

    // Create mask and extract
    let mask = (1u16 << bit_depth) - 1;
    (raw >> bit_offset) & mask
}
```

### Worked Example: Extracting Pixel 5 from 2-bit Bitmap

```
Given: bit_depth = 2, pixel_idx = 5

Step 1: Calculate position
  bit_pos = 5 × 2 = 10
  byte_idx = 10 / 8 = 1
  bit_offset = 10 % 8 = 2

Step 2: Read bytes (assume bitmap[0]=0xE4, bitmap[1]=0x1B)
  byte 0: 0xE4 = 1110 0100
  byte 1: 0x1B = 0001 1011

Step 3: Combine as 16-bit value
  raw = 0xE4 | (0x1B << 8) = 0x1BE4
  binary: 0001 1011 1110 0100

Step 4: Shift and mask
  shifted = 0x1BE4 >> 2 = 0x06F9
  binary: 0000 0110 1111 1001

  mask = (1 << 2) - 1 = 0x03 = 0000 0000 0000 0011

  result = 0x06F9 & 0x03 = 0x01

Pixel 5 has palette index 1
```

### Visual: 2-bit Bitmap Layout

```
Bitmap for 32×32 chunk at 2 bits per pixel (256 bytes total):

Byte    Bits          Pixels (row, col within chunk)
────    ────────────  ─────────────────────────────────
0       [7:6][5:4][3:2][1:0]    (0,3) (0,2) (0,1) (0,0)
1       [7:6][5:4][3:2][1:0]    (0,7) (0,6) (0,5) (0,4)
2       [7:6][5:4][3:2][1:0]    (0,11)(0,10)(0,9) (0,8)
...
7       [7:6][5:4][3:2][1:0]    (0,31)(0,30)(0,29)(0,28)
8       [7:6][5:4][3:2][1:0]    (1,3) (1,2) (1,1) (1,0)   ← Row 1 starts
...
255     [7:6][5:4][3:2][1:0]    (31,31)(31,30)(31,29)(31,28)
```

### Palette vs Raw Values

The bitmap values are interpreted differently based on bit_depth:

```
if bit_depth <= 2 && palette_count > 0:
    // Bitmap contains palette indices
    pixel_value = palette[bitmap_value]
else:
    // Bitmap contains raw values (no lookup)
    pixel_value = bitmap_value
```

Example with palette:

```
palette = [0, 33, 65, 131]  // 4 entries
bitmap_value = 2
pixel_value = palette[2] = 65
```

Example without palette (bit_depth > 2):

```
bit_depth = 5
palette_count = 0
bitmap_value = 17
pixel_value = 17  // Used directly
```

---

## 8. The Complete Decoding Algorithm

### Pseudocode

```
function decode_gdm(file_data):
    // Step 1: Parse header
    magic = file_data[0:4]
    if magic == "!MDF":
        header_size = 9
        dimension = 2^(file_data[4] + 5)
        chunk_size = 2^file_data[5]
        num_channels = file_data[7]
        num_compression_ranges = file_data[8]
    else if magic == '"MDF':
        header_size = 16
        dimension = 2^(file_data[8] + 5)
        chunk_size = 2^file_data[9]
        num_channels = file_data[11]
        num_compression_ranges = file_data[12]

    // Step 2: Calculate derived values
    chunks_per_dim = dimension / chunk_size
    total_chunks = chunks_per_dim × chunks_per_dim

    // Step 3: Read compression boundaries
    boundaries = [0]
    for i in 0..(num_compression_ranges - 1):
        boundaries.append(file_data[header_size + i])
    boundaries.append(num_channels)

    // Calculate bits per range
    bits_per_range = []
    for i in 0..num_compression_ranges:
        bits_per_range.append(boundaries[i+1] - boundaries[i])

    // Step 4: Calculate data start
    boundaries_size = max(0, num_compression_ranges - 1)
    data_start = header_size + boundaries_size

    // Step 5: Determine output format
    use_rgb = (num_channels > 8)

    // Step 6: Create output image
    if use_rgb:
        image = new RGB_Image(dimension, dimension)
    else:
        image = new Grayscale_Image(dimension, dimension)

    // Step 7: Decode all blocks
    pos = data_start
    for chunk_idx in 0..total_chunks:
        // Decode all ranges for this chunk
        range_values = []
        for range_idx in 0..num_compression_ranges:
            (pixels, block_size) = decode_block(file_data, pos, chunk_size)
            range_values.append(pixels)
            pos += block_size

        // Calculate chunk position in image
        chunk_row = chunk_idx / chunks_per_dim
        chunk_col = chunk_idx % chunks_per_dim
        base_x = chunk_col × chunk_size
        base_y = chunk_row × chunk_size

        // Combine ranges and write pixels
        for pixel_idx in 0..1024:
            combined = 0
            shift = 0
            for (range_idx, pixels) in enumerate(range_values):
                combined |= pixels[pixel_idx] << shift
                shift += bits_per_range[range_idx]

            px = pixel_idx % chunk_size
            py = pixel_idx / chunk_size
            x = base_x + px
            y = base_y + py

            if use_rgb:
                r = combined & 0xFF
                g = (combined >> 8) & 0xFF
                b = (combined >> 16) & 0xFF
                image.set_pixel(x, y, (r, g, b))
            else:
                image.set_pixel(x, y, combined & 0xFF)

    return image


function decode_block(data, pos, chunk_size):
    bit_depth = data[pos]
    palette_count = data[pos + 1]

    // Read palette
    palette = []
    for i in 0..palette_count:
        value = data[pos + 2 + i*2] | (data[pos + 3 + i*2] << 8)
        palette.append(value)

    bitmap_size = bit_depth × 128
    block_size = 2 + 2×palette_count + bitmap_size

    total_pixels = chunk_size × chunk_size  // 1024

    if bit_depth == 0:
        // Uniform block
        return (array of palette[0] repeated total_pixels times, block_size)

    // Decode bitmap
    bitmap = data[pos + 2 + 2×palette_count : pos + block_size]
    pixels = []

    for pixel_idx in 0..total_pixels:
        bit_pos = pixel_idx × bit_depth
        byte_idx = bit_pos / 8
        bit_offset = bit_pos % 8

        raw = bitmap[byte_idx]
        if byte_idx + 1 < len(bitmap):
            raw |= bitmap[byte_idx + 1] << 8

        mask = (1 << bit_depth) - 1
        index = (raw >> bit_offset) & mask

        if bit_depth <= 2 and palette_count > 0:
            value = palette[index]
        else:
            value = index

        pixels.append(value)

    return (pixels, block_size)
```

### State Machine View

```
                        ┌──────────────┐
                        │    START     │
                        └──────┬───────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │   Parse Header       │
                    │   - Detect format    │
                    │   - Read dimensions  │
                    │   - Read channels    │
                    └──────────┬───────────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │ Read Boundaries      │
                    │ (if ranges > 1)      │
                    └──────────┬───────────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │ Initialize Position  │
                    │ pos = data_start     │
                    │ chunk = 0            │
                    └──────────┬───────────┘
                               │
                               ▼
              ┌────────────────────────────────────┐
              │                                    │
              ▼                                    │
    ┌───────────────────┐                         │
    │ chunk < total?    │───── No ──────────►┌────┴────┐
    └─────────┬─────────┘                    │  DONE   │
              │ Yes                          └─────────┘
              ▼
    ┌───────────────────┐
    │ range = 0         │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ range < total?    │───── No ─────┐
    └─────────┬─────────┘              │
              │ Yes                    │
              ▼                        │
    ┌───────────────────┐              │
    │ Decode Block      │              │
    │ at position       │              │
    └─────────┬─────────┘              │
              │                        │
              ▼                        │
    ┌───────────────────┐              │
    │ Store pixels      │              │
    │ pos += block_size │              │
    │ range++           │              │
    └─────────┬─────────┘              │
              │                        │
              └───────────►┌───────────┘
                           │
                           ▼
              ┌───────────────────────┐
              │ Combine ranges        │
              │ Write to image        │
              │ chunk++               │
              └───────────┬───────────┘
                          │
                          └─────────────────►(loop back)
```

---

## 9. PNG Output Generation

### Output Format Selection

The output format is determined solely by the number of channels:

```
if num_channels <= 8:
    output = Grayscale (8-bit)
    // All channel bits fit in one byte
else:
    output = RGB (24-bit)
    // Channel bits span multiple bytes
```

### Grayscale Mapping (≤8 channels)

```
Combined value (1-8 bits) → Grayscale pixel (8 bits)

Example: 4 channels
  combined = 0b1011 = 11
  pixel = 11 (grayscale value 0-255, but only 0-15 used)

The pixel directly uses the combined value.
```

### RGB Mapping (>8 channels)

```
Combined value (9-24 bits) → RGB pixel

Bit assignment:
  R = bits 0-7   (always full)
  G = bits 8-15  (may be partially used)
  B = bits 16-23 (rarely used)

Example: 10 channels
  combined = 0b01_0010_0001 = 289
  R = 289 & 0xFF = 33      (bits 0-7)
  G = (289 >> 8) & 0xFF = 1  (bits 8-9, values 0-3)
  B = 0                     (no bits in range)

Example: 12 channels
  combined = 0b1111_0010_0001 = 3873
  R = 3873 & 0xFF = 33
  G = (3873 >> 8) & 0xFF = 15
  B = 0
```

### Visual: 10-bit to RGB Conversion

```
         10-bit Combined Value
    ┌────────────────────────────────┐
    │ 0 │ 1 │ 0 │ 0 │ 1 │ 0 │ 0 │ 0 │ 0 │ 1 │
    └────────────────────────────────┘
      b9  b8  b7  b6  b5  b4  b3  b2  b1  b0
      └───┘   └────────────────────────────┘
        G                  R
      (2 bits)          (8 bits)

    R = 0b00100001 = 33
    G = 0b01 = 1
    B = 0

    RGB = (33, 1, 0)
```

### File Type to PNG Format Table

| File Type      | Channels | Ranges | Total Bits | PNG Format | R bits | G bits | B bits |
| -------------- | -------- | ------ | ---------- | ---------- | ------ | ------ | ------ |
| stones         | 3        | 1      | 3          | Grayscale  | -      | -      | -      |
| weed           | 4        | 1      | 4          | Grayscale  | -      | -      | -      |
| groundFoliage  | 4        | 1      | 4          | Grayscale  | -      | -      | -      |
| fruits (US/EU) | 10       | 2      | 10         | RGB        | 8      | 2      | 0      |
| fruits (AS)    | 11       | 2      | 11         | RGB        | 8      | 3      | 0      |
| ground         | 11       | 1      | 11         | RGB        | 8      | 3      | 0      |
| height         | 12       | 2      | 12         | RGB        | 8      | 4      | 0      |

---

## 10. Real-World File Analysis

### File: mapUS_densityMap_fruits.gdm

```
=== HEADER ANALYSIS ===

Offset  Hex                               Meaning
──────  ────────────────────────────────  ─────────────────────────────
0x0000  21 4D 44 46                       Magic: "!MDF"
0x0004  07                                dim_log2=7 → 4096×4096
0x0005  05                                chunk_log2=5 → 32×32 chunks
0x0006  02                                max_bpp=2
0x0007  0A                                num_channels=10
0x0008  02                                num_compression_ranges=2

0x0009  05                                Boundary: channel 5
                                          Range 0: ch 0-4 (5 bits)
                                          Range 1: ch 5-9 (5 bits)

Data starts at offset 0x000A (10)

=== STATISTICS ===

Total file size:     8,032,024 bytes
Header + boundaries: 10 bytes
Block data:          8,032,014 bytes

Image dimensions:    4096 × 4096 = 16,777,216 pixels
Chunk count:         128 × 128 = 16,384 chunks
Block count:         16,384 × 2 = 32,768 blocks

Average bytes/block: 245 bytes
Average bytes/pixel: 0.48 bytes (vs 2 bytes uncompressed)
Compression ratio:   4.2:1

=== BLOCK DISTRIBUTION ===

bit_depth=0 (uniform):    10,022 blocks (30.6%)
bit_depth=1 (2 values):    4,772 blocks (14.6%)
bit_depth=2 (4 values):   11,097 blocks (33.9%)
bit_depth=5 (32 values):   6,877 blocks (21.0%)

=== FIRST 10 BLOCKS ===

Block  Offset   BD  PC  Palette              Size
─────  ───────  ──  ──  ───────────────────  ────
0      10       2   3   [0, 1, 3]            264
1      274      2   4   [0, 1, 9, 4]         266
2      540      2   3   [0, 1, 3]            264
3      804      2   4   [0, 1, 9, 4]         266
4      1070     2   3   [0, 1, 3]            264
5      1334     2   4   [0, 9, 4, 1]         266
6      1600     2   3   [1, 0, 3]            264
7      1864     2   4   [1, 0, 9, 4]         266
8      2130     2   3   [0, 1, 3]            264
9      2394     2   4   [0, 1, 9, 4]         266

=== SAMPLE PIXEL DECODING ===

Chunk 0, Pixel 8 (coordinates: x=8, y=0):

Block 0 (Range 0):
  bit_depth=2, palette=[0, 1, 3]
  Bitmap extraction: index=1, value=palette[1]=1

Block 1 (Range 1):
  bit_depth=2, palette=[0, 1, 9, 4]
  Bitmap extraction: index=2, value=palette[2]=9

Combined: 1 | (9 << 5) = 1 | 288 = 289
Binary: 0b0100100001

RGB Output:
  R = 289 & 0xFF = 33
  G = (289 >> 8) & 0xFF = 1
  B = 0

Final pixel: RGB(33, 1, 0) ✓

=== OFFICIAL TOOL COMPARISON ===

Our decoder output: /tmp/test_fruits.png
Official output:    mapUS_densityMap_fruits.png

Result: 0 pixel differences (PERFECT MATCH)
```

### File: mapAS_densityMap_stones.gdm (0x22 Format)

```
=== HEADER ANALYSIS ===

Offset  Hex                               Meaning
──────  ────────────────────────────────  ─────────────────────────────
0x0000  22 4D 44 46                       Magic: '"MDF'
0x0004  00 00 00 00                       Version: 0
0x0008  07                                dim_log2=7 → 4096×4096
0x0009  05                                chunk_log2=5 → 32×32 chunks
0x000A  02                                max_bpp=2
0x000B  03                                num_channels=3
0x000C  01                                num_compression_ranges=1
0x000D  00                                type_index_channels=0
0x000E  00 00                             Reserved

Data starts at offset 0x0010 (16)

=== STATISTICS ===

Total file size:     2,109,152 bytes
Image dimensions:    4096 × 4096
Chunk count:         16,384 chunks
Block count:         16,384 blocks

=== OUTPUT ===

Format: Grayscale (3 channels ≤ 8)
Pixel values: 0-7 (3 bits)

Result: 0 pixel differences vs official tool
```

---

## 11. Compression Analysis

### How Efficient Is This Format?

The GDM format achieves compression through:

1. **Chunk-based encoding**: Each chunk uses optimal bit depth
2. **Palette compression**: Map many values to few indices
3. **Uniform chunk optimization**: Store constant regions in 4 bytes

### Compression Scenarios

```
┌────────────────────────────────────────────────────────────────────────────┐
│                     COMPRESSION EFFICIENCY                                 │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  Scenario 1: Uniform Chunk (all pixels same value)                         │
│  ─────────────────────────────────────────────────                         │
│  Raw: 1024 pixels × 2 bytes = 2048 bytes                                   │
│  GDM: 2 (header) + 2 (palette[0]) = 4 bytes                                │
│  Ratio: 512:1                                                              │
│                                                                            │
│  Scenario 2: Two Unique Values                                             │
│  ─────────────────────────────────────────────                             │
│  Raw: 2048 bytes                                                           │
│  GDM: 2 + 4 (palette) + 128 (bitmap) = 134 bytes                           │
│  Ratio: 15:1                                                               │
│                                                                            │
│  Scenario 3: Four Unique Values                                            │
│  ─────────────────────────────────────────────                             │
│  Raw: 2048 bytes                                                           │
│  GDM: 2 + 8 (palette) + 256 (bitmap) = 266 bytes                           │
│  Ratio: 7.7:1                                                              │
│                                                                            │
│  Scenario 4: Complex Chunk (32 unique values)                              │
│  ─────────────────────────────────────────────                             │
│  Raw: 2048 bytes                                                           │
│  GDM: 2 + 0 (no palette) + 640 (bitmap) = 642 bytes                        │
│  Ratio: 3.2:1                                                              │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

### Real-World Compression Results

```
File                              Raw Size    GDM Size    Ratio
────────────────────────────────  ──────────  ──────────  ─────
mapUS_densityMap_fruits.gdm       33.5 MB     8.0 MB      4.2:1
mapUS_densityMap_stones.gdm       33.5 MB     2.1 MB      16:1
mapUS_densityMap_weed.gdm         33.5 MB     0.06 MB     558:1
mapUS_densityMap_ground.gdm       33.5 MB     0.06 MB     558:1
mapUS_densityMap_groundFoliage.gdm 8.4 MB     0.7 MB      12:1
mapUS_densityMap_height.gdm       33.5 MB     0.13 MB     258:1
```

The weed and ground files achieve extreme compression because most of the terrain is empty/uniform.

---

## 12. Implementation Guide

### Rust Implementation

See the complete implementation in `src/bin/gdm_to_png.rs`:

```rust
// Key data structures

struct GdmHeader {
    dimension: usize,
    chunk_size: usize,
    num_channels: usize,
    num_compression_ranges: usize,
}

struct Block {
    bit_depth: u8,
    palette: Vec<u16>,
    bitmap: Vec<u8>,
}

// Core decoding function
fn decode_block_pixels(data: &[u8], pos: usize, chunk_size: usize)
    -> (Vec<u16>, usize)
{
    let bit_depth = data[pos];
    let palette_count = data[pos + 1] as usize;
    let palette_size = 2 * palette_count;
    let bitmap_size = if bit_depth > 0 {
        (bit_depth as usize) * 128
    } else {
        0
    };
    let block_size = 2 + palette_size + bitmap_size;

    // Read palette
    let palette: Vec<u16> = (0..palette_count)
        .map(|i| u16::from_le_bytes([
            data[pos + 2 + i*2],
            data[pos + 3 + i*2]
        ]))
        .collect();

    let total_pixels = chunk_size * chunk_size;
    let mut pixels = Vec::with_capacity(total_pixels);

    if bit_depth == 0 {
        // Uniform chunk
        let value = *palette.first().unwrap_or(&0);
        pixels.resize(total_pixels, value);
    } else {
        // Decode bitmap
        let bitmap = &data[pos + 2 + palette_size..
                          pos + 2 + palette_size + bitmap_size];
        let mask = (1u16 << bit_depth) - 1;

        for pixel_idx in 0..total_pixels {
            let bit_pos = pixel_idx * (bit_depth as usize);
            let byte_idx = bit_pos / 8;
            let bit_offset = bit_pos % 8;

            let mut raw_value = bitmap[byte_idx] as u16;
            if byte_idx + 1 < bitmap.len() {
                raw_value |= (bitmap[byte_idx + 1] as u16) << 8;
            }

            let idx = ((raw_value >> bit_offset) & mask) as usize;

            let pixel_value = if bit_depth <= 2 && !palette.is_empty() {
                *palette.get(idx).unwrap_or(&0)
            } else {
                idx as u16
            };

            pixels.push(pixel_value);
        }
    }

    (pixels, block_size)
}
```

### Python Implementation Sketch

```python
import struct
from PIL import Image

def decode_gdm(filename):
    with open(filename, 'rb') as f:
        data = f.read()

    magic = data[0:4]

    if magic == b'!MDF':
        dim_log2 = data[4]
        chunk_log2 = data[5]
        num_channels = data[7]
        num_ranges = data[8]
        header_size = 9
    elif magic == b'"MDF':
        dim_log2 = data[8]
        chunk_log2 = data[9]
        num_channels = data[11]
        num_ranges = data[12]
        header_size = 16

    dimension = 1 << (dim_log2 + 5)
    chunk_size = 1 << chunk_log2
    chunks_per_dim = dimension // chunk_size
    total_chunks = chunks_per_dim ** 2

    # Read compression boundaries
    boundaries = [0]
    for i in range(num_ranges - 1):
        boundaries.append(data[header_size + i])
    boundaries.append(num_channels)

    bits_per_range = [boundaries[i+1] - boundaries[i]
                      for i in range(num_ranges)]

    data_start = header_size + max(0, num_ranges - 1)

    # Create output image
    use_rgb = num_channels > 8
    if use_rgb:
        image = Image.new('RGB', (dimension, dimension))
    else:
        image = Image.new('L', (dimension, dimension))

    pixels = image.load()
    pos = data_start

    for chunk_idx in range(total_chunks):
        range_values = []

        for _ in range(num_ranges):
            chunk_pixels, block_size = decode_block(
                data, pos, chunk_size)
            range_values.append(chunk_pixels)
            pos += block_size

        chunk_row = chunk_idx // chunks_per_dim
        chunk_col = chunk_idx % chunks_per_dim
        base_x = chunk_col * chunk_size
        base_y = chunk_row * chunk_size

        for pixel_idx in range(chunk_size * chunk_size):
            combined = 0
            shift = 0
            for range_idx, rng_pixels in enumerate(range_values):
                combined |= rng_pixels[pixel_idx] << shift
                shift += bits_per_range[range_idx]

            px = pixel_idx % chunk_size
            py = pixel_idx // chunk_size
            x = base_x + px
            y = base_y + py

            if use_rgb:
                r = combined & 0xFF
                g = (combined >> 8) & 0xFF
                b = (combined >> 16) & 0xFF
                pixels[x, y] = (r, g, b)
            else:
                pixels[x, y] = combined & 0xFF

    return image


def decode_block(data, pos, chunk_size):
    bit_depth = data[pos]
    palette_count = data[pos + 1]

    palette = []
    for i in range(palette_count):
        val = struct.unpack_from('<H', data, pos + 2 + i*2)[0]
        palette.append(val)

    bitmap_size = bit_depth * 128
    block_size = 2 + 2*palette_count + bitmap_size
    total_pixels = chunk_size * chunk_size

    if bit_depth == 0:
        return [palette[0] if palette else 0] * total_pixels, block_size

    bitmap = data[pos + 2 + 2*palette_count :
                  pos + 2 + 2*palette_count + bitmap_size]

    pixels = []
    mask = (1 << bit_depth) - 1

    for pixel_idx in range(total_pixels):
        bit_pos = pixel_idx * bit_depth
        byte_idx = bit_pos // 8
        bit_offset = bit_pos % 8

        raw = bitmap[byte_idx]
        if byte_idx + 1 < len(bitmap):
            raw |= bitmap[byte_idx + 1] << 8

        idx = (raw >> bit_offset) & mask

        if bit_depth <= 2 and palette:
            value = palette[idx] if idx < len(palette) else 0
        else:
            value = idx

        pixels.append(value)

    return pixels, block_size
```

---

## Appendix: Hex Dumps

### mapUS_densityMap_fruits.gdm - First 256 Bytes

```
00000000: 21 4d 44 46 07 05 02 0a  02 05 02 03 00 00 01 00  |!MDF............|
00000010: 03 00 50 50 05 0a 00 00  00 00 50 50 05 0a 00 00  |..PP......PP....|
00000020: 00 00 0a 05 50 00 50 00  00 00 0a 05 50 00 50 00  |....P.P.....P.P.|
00000030: 00 00 55 05 00 50 50 05  00 40 55 05 00 50 50 05  |..U..PP..@U..PP.|
00000040: 00 50 50 00 05 00 00 55  01 04 50 00 05 00 00 55  |.PP....U..P....U|
00000050: 55 05 05 55 00 55 50 55  05 05 05 55 00 55 50 55  |U..U.UPU...U.UPU|
00000060: 05 05 05 55 55 50 00 05  a0 00 05 55 55 50 00 05  |...UUP.....UUP..|
00000070: a0 00 00 0a 50 00 05 a0  50 50 00 0a 50 00 05 a0  |....P...PP..P...|
00000080: 50 50 00 55 05 55 05 05  50 00 00 55 05 55 05 05  |PP.U.U..P..U.U..|
00000090: 50 00 55 50 50 05 00 05  50 05 55 50 50 05 00 05  |P.UPP...P.UPP...|
000000a0: 50 05 55 50 50 05 00 05  50 05 55 50 50 05 00 05  |P.UPP...P.UPP...|
000000b0: 50 05 55 50 50 05 00 05  50 05 55 50 50 05 00 05  |P.UPP...P.UPP...|

Breakdown:
  Bytes 0-8: Header (magic + dimensions + channels + ranges)
  Byte 9: Compression boundary (0x05 = channel 5)
  Bytes 10+: Block data begins

First block (offset 10):
  Byte 10: 0x02 = bit_depth (2 bits per pixel)
  Byte 11: 0x03 = palette_count (3 entries)
  Bytes 12-17: palette = [0x0000, 0x0001, 0x0003]
  Bytes 18-273: bitmap (256 bytes)
```

### mapAS_densityMap_stones.gdm - First 64 Bytes

```
00000000: 22 4d 44 46 00 00 00 00  07 05 02 03 01 00 00 00  |"MDF............|
00000010: 01 02 00 00 02 00 00 00  00 00 00 00 00 01 04 00  |................|
00000020: 01 02 00 00 02 00 00 00  00 00 00 00 00 01 04 00  |................|
00000030: 01 02 00 00 02 00 00 00  00 00 00 00 00 01 04 00  |................|

Breakdown:
  Bytes 0-3: Magic '"MDF' (0x22 variant)
  Bytes 4-7: Version = 0
  Byte 8: dim_log2 = 7 → 4096
  Byte 9: chunk_log2 = 5 → 32
  Byte 10: max_bpp = 2
  Byte 11: num_channels = 3
  Byte 12: num_compression_ranges = 1
  Byte 13: type_index_channels = 0
  Bytes 14-15: Reserved (0x0000)

Data starts at byte 16:
  First block at offset 16:
    Byte 16: 0x01 = bit_depth (1 bit per pixel)
    Byte 17: 0x02 = palette_count (2 entries)
    Bytes 18-21: palette = [0x0000, 0x0002]
    Bytes 22-149: bitmap (128 bytes)
```

---

## Summary

The GDM format is a clever compression scheme optimized for terrain data:

1. **Chunk-based**: 32×32 pixel tiles enable spatial locality
2. **Adaptive bit depth**: Use only as many bits as needed
3. **Palette compression**: Map few unique values efficiently
4. **Compression ranges**: Split channels for better compression
5. **Uniform optimization**: Constant areas store in 4 bytes

This documentation was created through reverse-engineering and has been verified to produce bit-perfect output compared to the official GIANTS converter tool.

---

_Document version: 1.0_
_Created: 2024_
_Format verified against: grleConverter.exe from Farming Simulator 25_
_Test results: 0 pixel differences across all test files_
