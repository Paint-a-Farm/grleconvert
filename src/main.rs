use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

// ============================================================================
// I3D Parsing for encoding parameters
// ============================================================================

#[derive(Debug, Clone)]
struct LayerParams {
    layer_type: LayerType,
    num_channels: usize,
    compression_channels: Option<usize>, // For GDM with multiple ranges
}

#[derive(Debug, Clone, PartialEq)]
enum LayerType {
    InfoLayer,  // GRLE
    GdmLayer,   // GDM (DetailLayer or FoliageMultiLayer)
}

/// Find i3d file by walking up the directory hierarchy
fn find_i3d_file(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()?.to_path_buf()
    } else {
        start_path.to_path_buf()
    };

    loop {
        // Look for *.i3d in current directory
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("i3d") {
                    return Some(path);
                }
            }
        }

        // Move up one directory
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    None
}

/// Parse i3d file to find layer parameters for a given filename
fn parse_i3d_for_file(i3d_path: &Path, target_filename: &str) -> Option<LayerParams> {
    let content = std::fs::read_to_string(i3d_path).ok()?;

    // Step 1: Find fileId for the target filename (match basename, ignore path prefix like $data/)
    let target_basename = Path::new(target_filename)
        .file_name()?
        .to_str()?;

    // Also try with .png extension since i3d references PNG files
    let target_png = if target_basename.ends_with(".png") {
        target_basename.to_string()
    } else {
        let stem = Path::new(target_basename).file_stem()?.to_str()?;
        format!("{}.png", stem)
    };

    eprintln!("Looking for file: {}", target_png);

    // Find <File fileId="NNN" filename="...target_png"/>
    let mut file_id: Option<&str> = None;
    for line in content.lines() {
        if line.contains("<File") && line.contains(&target_png) {
            // Extract fileId
            if let Some(start) = line.find("fileId=\"") {
                let rest = &line[start + 8..];
                if let Some(end) = rest.find('"') {
                    file_id = Some(&rest[..end]);
                    eprintln!("Found fileId: {}", &rest[..end]);
                    break;
                }
            }
        }
    }

    let file_id = file_id?;

    // Step 2: Find layer definition that references this fileId
    // Check InfoLayer (GRLE)
    for line in content.lines() {
        if line.contains("<InfoLayer") && line.contains(&format!("fileId=\"{}\"", file_id)) {
            // Extract numChannels
            if let Some(num_channels) = extract_attr(line, "numChannels") {
                eprintln!("Found InfoLayer with {} channels → GRLE", num_channels);
                return Some(LayerParams {
                    layer_type: LayerType::InfoLayer,
                    num_channels,
                    compression_channels: None,
                });
            }
        }
    }

    // Check DetailLayer (GDM) - uses densityMapId
    for line in content.lines() {
        if line.contains("<DetailLayer") && line.contains(&format!("densityMapId=\"{}\"", file_id)) {
            let num_channels = extract_attr(line, "numDensityMapChannels")?;
            let compression_channels = extract_attr(line, "compressionChannels");
            eprintln!("Found DetailLayer with {} channels, compression: {:?} → GDM",
                     num_channels, compression_channels);
            return Some(LayerParams {
                layer_type: LayerType::GdmLayer,
                num_channels,
                compression_channels,
            });
        }
    }

    // Check FoliageMultiLayer (GDM) - uses densityMapId
    for line in content.lines() {
        if line.contains("<FoliageMultiLayer") && line.contains(&format!("densityMapId=\"{}\"", file_id)) {
            let num_channels = extract_attr(line, "numChannels")?;
            let compression_channels = extract_attr(line, "compressionChannels");
            eprintln!("Found FoliageMultiLayer with {} channels, compression: {:?} → GDM",
                     num_channels, compression_channels);
            return Some(LayerParams {
                layer_type: LayerType::GdmLayer,
                num_channels,
                compression_channels,
            });
        }
    }

    None
}

fn extract_attr(line: &str, attr_name: &str) -> Option<usize> {
    let pattern = format!("{}=\"", attr_name);
    if let Some(start) = line.find(&pattern) {
        let rest = &line[start + pattern.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].parse().ok();
        }
    }
    None
}

// ============================================================================
// Utility functions
// ============================================================================

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

// ============================================================================
// GRLE Decoder
// ============================================================================

fn decode_grle_rle(data: &[u8], expected_size: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(expected_size);
    let mut i = 1; // Skip first byte (0x00 flag/padding)

    while i + 1 < data.len() && output.len() < expected_size {
        let prev = data[i];
        let new_val = data[i + 1];
        i += 2;

        if prev == new_val {
            // Same value: read extended count with 0xff continuation
            let mut count = 0usize;
            while i < data.len() && data[i] == 0xff {
                count += 255;
                i += 1;
            }
            if i < data.len() {
                count += data[i] as usize;
                i += 1;
            }
            count += 2; // Counts are offset by 2

            let to_emit = count.min(expected_size - output.len());
            output.extend(std::iter::repeat(prev).take(to_emit));
        } else {
            // Transition: emit 1 pixel of prev, back up to re-read new as next prev
            output.push(prev);
            i -= 1;
        }
    }

    output.resize(expected_size, 0);
    output
}

fn convert_grle_to_png(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufReader::new(File::open(input_path)?);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    if &data[0..4] != b"GRLE" {
        return Err("Not a valid GRLE file".into());
    }

    let version = read_u16_le(&data, 4);
    let width = (read_u16_le(&data, 6) as usize) * 256;
    let height = (read_u16_le(&data, 10) as usize) * 256;
    let channels = 1usize;

    eprintln!("GRLE version: {}", version);
    eprintln!("Size: {}x{}", width, height);
    eprintln!("Channels: {}", channels);

    let compressed_data = &data[20..];
    let expected_size = width * height * channels;

    let pixels = decode_grle_rle(compressed_data, expected_size);

    let file = File::create(output_path)?;
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, width as u32, height as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Default);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;

    eprintln!("Saved to {}", output_path);
    Ok(())
}

// ============================================================================
// GRLE Encoder
// ============================================================================

fn encode_grle_rle(pixels: &[u8]) -> Vec<u8> {
    // GRLE RLE format:
    // - Initial 0x00 byte (padding/flag)
    // - Decoder reads pairs (data[i], data[i+1]):
    //   - If same: run - read count bytes (0xff=+255, final byte=remainder), emit count+2 pixels
    //   - If different: transition - emit first pixel, back up 1 byte
    //
    // Each pixel value appears once in the stream, except runs which have value twice + count.

    let mut output = Vec::new();
    output.push(0x00);

    if pixels.is_empty() {
        return output;
    }

    let mut i = 0;
    while i < pixels.len() {
        let value = pixels[i];

        // Count consecutive identical values
        let mut run_len = 1;
        while i + run_len < pixels.len() && pixels[i + run_len] == value {
            run_len += 1;
        }

        if run_len >= 2 {
            // Run: emit (value, value, count) where count = run_len - 2
            output.push(value);
            output.push(value);

            let mut remaining = run_len - 2;
            while remaining >= 255 {
                output.push(0xff);
                remaining -= 255;
            }
            output.push(remaining as u8);

            i += run_len;
        } else {
            // Single pixel - emit value, decoder handles via transition backup
            output.push(value);
            i += 1;
        }
    }

    // Edge case: single pixel image needs padding for decoder
    if output.len() == 2 {
        let v = output[1];
        output.push(v);
        output.push(0x00);
    }

    output
}

fn convert_png_to_grle(input_path: &str, output_path: &str, params: &LayerParams) -> Result<(), Box<dyn std::error::Error>> {
    // Read PNG
    let file = File::open(input_path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info()?;

    let mut pixels = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;

    let width = info.width as usize;
    let height = info.height as usize;

    eprintln!("PNG: {}x{}", width, height);
    eprintln!("Encoding as GRLE with {} channels", params.num_channels);

    // GRLE dimensions must be multiples of 256
    if width % 256 != 0 || height % 256 != 0 {
        return Err(format!("Dimensions must be multiples of 256, got {}x{}", width, height).into());
    }

    // Convert to grayscale if needed
    let grayscale_pixels = match info.color_type {
        png::ColorType::Grayscale => pixels[..width * height].to_vec(),
        png::ColorType::Rgb => {
            let mut gray = Vec::with_capacity(width * height);
            for chunk in pixels.chunks(3) {
                gray.push(chunk[0]); // Just use R channel
            }
            gray
        }
        png::ColorType::Rgba => {
            let mut gray = Vec::with_capacity(width * height);
            for chunk in pixels.chunks(4) {
                gray.push(chunk[0]); // Just use R channel
            }
            gray
        }
        _ => return Err("Unsupported PNG color type".into()),
    };

    // Encode RLE
    let compressed = encode_grle_rle(&grayscale_pixels);

    // Build GRLE file
    // Header format (20 bytes):
    // 0-3:   Magic "GRLE"
    // 4-5:   Version (1)
    // 6-7:   Width / 256
    // 8-9:   Padding (0)
    // 10-11: Height / 256
    // 12-13: Unknown (1) - possibly channels or bits
    // 14-15: Padding (0)
    // 16-19: Compressed size
    let mut output = Vec::new();

    // Magic
    output.extend_from_slice(b"GRLE");

    // Version (1)
    output.extend_from_slice(&1u16.to_le_bytes());

    // Width / 256
    output.extend_from_slice(&((width / 256) as u16).to_le_bytes());

    // Padding
    output.extend_from_slice(&[0u8; 2]);

    // Height / 256
    output.extend_from_slice(&((height / 256) as u16).to_le_bytes());

    // Unknown field (256) - seen in all GRLE files
    output.extend_from_slice(&256u16.to_le_bytes());

    // Padding
    output.extend_from_slice(&[0u8; 2]);

    // Compressed size: stored as 0x00 followed by 3-byte LE value
    // Value stored is (compressed.len() - 1)
    let comp_size = (compressed.len() - 1) as u32;
    output.push(0x00);
    output.push((comp_size & 0xFF) as u8);
    output.push(((comp_size >> 8) & 0xFF) as u8);
    output.push(((comp_size >> 16) & 0xFF) as u8);

    // Compressed data
    output.extend_from_slice(&compressed);

    // Write file
    let mut file = File::create(output_path)?;
    file.write_all(&output)?;

    eprintln!("Saved to {} ({} bytes)", output_path, output.len());
    Ok(())
}

// ============================================================================
// GDM Decoder
// ============================================================================

fn decode_gdm_block(data: &[u8], pos: usize, chunk_size: usize) -> (Vec<u16>, usize) {
    let bit_depth = data[pos];
    let palette_count = data[pos + 1] as usize;
    let palette_size = 2 * palette_count;
    let bitmap_size = if bit_depth > 0 { (bit_depth as usize) * 128 } else { 0 };
    let block_size = 2 + palette_size + bitmap_size;

    let palette: Vec<u16> = (0..palette_count)
        .map(|i| u16::from_le_bytes([data[pos + 2 + i*2], data[pos + 3 + i*2]]))
        .collect();

    let total_pixels = chunk_size * chunk_size;
    let mut pixels = Vec::with_capacity(total_pixels);

    if bit_depth == 0 {
        let value = *palette.first().unwrap_or(&0);
        pixels.resize(total_pixels, value);
    } else {
        let bitmap = &data[pos + 2 + palette_size..pos + 2 + palette_size + bitmap_size];
        let bits_per_pixel = bit_depth as usize;
        let mask = (1u16 << bits_per_pixel) - 1;

        for pixel_idx in 0..total_pixels {
            let bit_pos = pixel_idx * bits_per_pixel;
            let byte_idx = bit_pos / 8;
            let bit_offset = bit_pos % 8;

            let mut raw_value = bitmap[byte_idx] as u16;
            if byte_idx + 1 < bitmap.len() {
                raw_value |= (bitmap[byte_idx + 1] as u16) << 8;
            }

            let idx_or_value = ((raw_value >> bit_offset) & mask) as usize;

            let pixel_value = if bit_depth <= 2 && !palette.is_empty() {
                *palette.get(idx_or_value).unwrap_or(&0)
            } else {
                idx_or_value as u16
            };

            pixels.push(pixel_value);
        }
    }

    (pixels, block_size)
}

fn convert_gdm_to_png(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufReader::new(File::open(input_path)?);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    if data.len() < 16 {
        return Err("File too small".into());
    }

    let magic = &data[0..4];
    if magic != b"\"MDF" && magic != b"!MDF" {
        return Err("Not a valid GDM file".into());
    }

    let (dimension, num_channels, chunk_size, num_compression_ranges, header_size) =
        if magic == b"\"MDF" {
            let version = read_u32_le(&data, 4);
            if version != 0 {
                return Err(format!("Unsupported GDM version: {}", version).into());
            }

            let dim_log2 = data[8] as usize;
            let chunk_log2 = data[9] as usize;
            let num_channels = data[11] as usize;
            let num_compression_ranges = data[12] as usize;

            let dimension = 1 << (dim_log2 + 5);
            let chunk_size = 1 << chunk_log2;

            (dimension, num_channels, chunk_size, num_compression_ranges, 16usize)
        } else {
            let dim_log2 = data[4] as usize;
            let chunk_log2 = data[5] as usize;
            let num_channels = data[7] as usize;
            let num_compression_ranges = data[8] as usize;

            let dimension = 1 << (dim_log2 + 5);
            let chunk_size = 1 << chunk_log2;

            (dimension, num_channels, chunk_size, num_compression_ranges, 9usize)
        };

    eprintln!("GDM: {}x{}, {} channels, {} compression ranges",
              dimension, dimension, num_channels, num_compression_ranges);

    let mut compression_boundaries = vec![0u8];
    for i in 0..(num_compression_ranges.saturating_sub(1)) {
        compression_boundaries.push(data[header_size + i]);
    }
    compression_boundaries.push(num_channels as u8);

    let mut bits_per_range = Vec::new();
    for i in 0..num_compression_ranges {
        let start_ch = compression_boundaries[i] as usize;
        let end_ch = compression_boundaries[i + 1] as usize;
        bits_per_range.push(end_ch - start_ch);
    }

    let chunks_per_dim = dimension / chunk_size;
    let total_chunks = chunks_per_dim * chunks_per_dim;

    let compression_boundaries_size = if num_compression_ranges > 1 { num_compression_ranges - 1 } else { 0 };
    let data_start = header_size + compression_boundaries_size;

    let use_rgb = num_channels > 8;

    let bytes_per_pixel = if use_rgb { 3 } else { 1 };
    let mut image = vec![0u8; dimension * dimension * bytes_per_pixel];

    let mut pos = data_start;

    for chunk_idx in 0..total_chunks {
        let mut range_values: Vec<Vec<u16>> = Vec::new();

        for _range_idx in 0..num_compression_ranges {
            if pos + 2 > data.len() {
                return Err("Unexpected end of data".into());
            }

            let (pixels, block_size) = decode_gdm_block(&data, pos, chunk_size);
            range_values.push(pixels);
            pos += block_size;
        }

        let chunk_row = chunk_idx / chunks_per_dim;
        let chunk_col = chunk_idx % chunks_per_dim;
        let base_y = chunk_row * chunk_size;
        let base_x = chunk_col * chunk_size;

        for pixel_idx in 0..(chunk_size * chunk_size) {
            let mut combined: u32 = 0;
            let mut shift = 0;
            for (range_idx, pixels) in range_values.iter().enumerate() {
                let val = pixels[pixel_idx] as u32;
                combined |= val << shift;
                shift += bits_per_range[range_idx];
            }

            let py = pixel_idx / chunk_size;
            let px = pixel_idx % chunk_size;
            let img_x = base_x + px;
            let img_y = base_y + py;

            if use_rgb {
                let r = (combined & 0xFF) as u8;
                let g = ((combined >> 8) & 0xFF) as u8;
                let b = ((combined >> 16) & 0xFF) as u8;
                let img_idx = (img_y * dimension + img_x) * 3;
                image[img_idx] = r;
                image[img_idx + 1] = g;
                image[img_idx + 2] = b;
            } else {
                let img_idx = img_y * dimension + img_x;
                image[img_idx] = (combined & 0xFF) as u8;
            }
        }
    }

    eprintln!("Data consumed: {} / {} bytes", pos, data.len());

    let file = File::create(output_path)?;
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, dimension as u32, dimension as u32);
    if use_rgb {
        encoder.set_color(png::ColorType::Rgb);
    } else {
        encoder.set_color(png::ColorType::Grayscale);
    }
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Default);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image)?;

    eprintln!("Saved to {}", output_path);
    Ok(())
}

// ============================================================================
// GDM Encoder
// ============================================================================

fn encode_gdm_block(pixels: &[u16], chunk_size: usize) -> Vec<u8> {
    let total_pixels = chunk_size * chunk_size;

    // Find unique values in this chunk
    let mut unique_values: Vec<u16> = pixels.iter().copied().collect();
    unique_values.sort_unstable();
    unique_values.dedup();

    let mut output = Vec::new();

    if unique_values.len() == 1 {
        // Uniform chunk: bit_depth = 0, palette_count = 1
        output.push(0u8); // bit_depth
        output.push(1u8); // palette_count
        output.extend_from_slice(&unique_values[0].to_le_bytes());
    } else if unique_values.len() <= 4 {
        // Use palette with bit_depth 1 or 2
        let bit_depth = if unique_values.len() <= 2 { 1 } else { 2 };
        let palette_count = unique_values.len();

        output.push(bit_depth);
        output.push(palette_count as u8);

        // Write palette
        for &val in &unique_values {
            output.extend_from_slice(&val.to_le_bytes());
        }

        // Create value to index mapping
        let value_to_idx: std::collections::HashMap<u16, usize> = unique_values
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, i))
            .collect();

        // Encode bitmap
        let bitmap_size = (bit_depth as usize) * 128;
        let mut bitmap = vec![0u8; bitmap_size];

        for (pixel_idx, &pixel) in pixels.iter().enumerate().take(total_pixels) {
            let idx = value_to_idx[&pixel];
            let bit_pos = pixel_idx * (bit_depth as usize);
            let byte_idx = bit_pos / 8;
            let bit_offset = bit_pos % 8;

            bitmap[byte_idx] |= (idx as u8) << bit_offset;
            if bit_offset + (bit_depth as usize) > 8 && byte_idx + 1 < bitmap.len() {
                bitmap[byte_idx + 1] |= (idx as u8) >> (8 - bit_offset);
            }
        }

        output.extend_from_slice(&bitmap);
    } else {
        // Need higher bit depth - find max value to determine bits needed
        let max_val = *unique_values.last().unwrap();
        let bit_depth = (16 - max_val.leading_zeros()).max(1) as u8;

        output.push(bit_depth);
        output.push(0u8); // No palette for high bit depths

        // Encode raw values in bitmap
        let bitmap_size = (bit_depth as usize) * 128;
        let mut bitmap = vec![0u8; bitmap_size];

        for (pixel_idx, &pixel) in pixels.iter().enumerate().take(total_pixels) {
            let bit_pos = pixel_idx * (bit_depth as usize);
            let byte_idx = bit_pos / 8;
            let bit_offset = bit_pos % 8;

            let val = pixel as u16;
            bitmap[byte_idx] |= (val << bit_offset) as u8;
            if byte_idx + 1 < bitmap.len() {
                bitmap[byte_idx + 1] |= (val >> (8 - bit_offset)) as u8;
            }
        }

        output.extend_from_slice(&bitmap);
    }

    output
}

fn convert_png_to_gdm(input_path: &str, output_path: &str, params: &LayerParams) -> Result<(), Box<dyn std::error::Error>> {
    // Read PNG
    let file = File::open(input_path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info()?;

    let mut pixels = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;

    let width = info.width as usize;
    let height = info.height as usize;

    if width != height {
        return Err("GDM requires square dimensions".into());
    }

    let dimension = width;

    // Calculate dimension log2 (dimension = 2^(dim_log2 + 5))
    let dim_log2 = (dimension.trailing_zeros() as usize).saturating_sub(5);
    if (1 << (dim_log2 + 5)) != dimension {
        return Err(format!("Dimension must be power of 2 >= 32, got {}", dimension).into());
    }

    let chunk_size = 32usize;
    let chunk_log2 = 5usize;
    let chunks_per_dim = dimension / chunk_size;

    let num_channels = params.num_channels;
    let compression_channels = params.compression_channels;

    eprintln!("PNG: {}x{}", width, height);
    eprintln!("Encoding as GDM with {} channels", num_channels);
    if let Some(cc) = compression_channels {
        eprintln!("Compression split at channel {}", cc);
    }

    // Determine number of compression ranges
    let (num_compression_ranges, bits_per_range): (usize, Vec<usize>) =
        if let Some(cc) = compression_channels {
            (2, vec![cc, num_channels - cc])
        } else {
            (1, vec![num_channels])
        };

    // Convert PNG pixels to channel values
    let _use_rgb = num_channels > 8;

    let channel_values: Vec<u32> = match info.color_type {
        png::ColorType::Grayscale => {
            pixels[..width * height].iter().map(|&v| v as u32).collect()
        }
        png::ColorType::Rgb => {
            let mut values = Vec::with_capacity(width * height);
            for chunk in pixels.chunks(3) {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                values.push(r | (g << 8) | (b << 16));
            }
            values
        }
        png::ColorType::Rgba => {
            let mut values = Vec::with_capacity(width * height);
            for chunk in pixels.chunks(4) {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                values.push(r | (g << 8) | (b << 16));
            }
            values
        }
        _ => return Err("Unsupported PNG color type".into()),
    };

    // Build GDM file
    let mut output = Vec::new();

    // Header ("MDF variant)
    output.extend_from_slice(b"\"MDF");
    output.extend_from_slice(&0u32.to_le_bytes()); // version
    output.push(dim_log2 as u8);
    output.push(chunk_log2 as u8);
    output.push(2u8); // max_bpp (expected <= 2 for palette mode)
    output.push(num_channels as u8);
    output.push(num_compression_ranges as u8);
    output.push(0u8); // type_index_channels
    output.extend_from_slice(&[0u8; 2]); // padding to 16 bytes

    // Compression boundaries (if more than 1 range)
    if num_compression_ranges > 1 {
        if let Some(cc) = compression_channels {
            output.push(cc as u8);
        }
    }

    // Encode chunks
    for chunk_idx in 0..(chunks_per_dim * chunks_per_dim) {
        let chunk_row = chunk_idx / chunks_per_dim;
        let chunk_col = chunk_idx % chunks_per_dim;
        let base_y = chunk_row * chunk_size;
        let base_x = chunk_col * chunk_size;

        // Extract pixel values for this chunk
        let mut chunk_pixels: Vec<u32> = Vec::with_capacity(chunk_size * chunk_size);
        for py in 0..chunk_size {
            for px in 0..chunk_size {
                let img_x = base_x + px;
                let img_y = base_y + py;
                chunk_pixels.push(channel_values[img_y * dimension + img_x]);
            }
        }

        // Encode each compression range
        let mut shift = 0;
        for range_idx in 0..num_compression_ranges {
            let range_bits = bits_per_range[range_idx];
            let mask = (1u32 << range_bits) - 1;

            // Extract range values from combined pixel values
            let range_pixels: Vec<u16> = chunk_pixels
                .iter()
                .map(|&v| ((v >> shift) & mask) as u16)
                .collect();

            let block = encode_gdm_block(&range_pixels, chunk_size);
            output.extend_from_slice(&block);

            shift += range_bits;
        }
    }

    // Write file
    let mut file = File::create(output_path)?;
    file.write_all(&output)?;

    eprintln!("Saved to {} ({} bytes)", output_path, output.len());
    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn print_usage() {
    eprintln!("Usage: grleconvert <input> [output]");
    eprintln!();
    eprintln!("Converts between GIANTS Engine density map formats and PNG.");
    eprintln!();
    eprintln!("Decoding (automatic):");
    eprintln!("  grleconvert input.gdm              → input.png");
    eprintln!("  grleconvert input.grle             → input.png");
    eprintln!();
    eprintln!("Encoding (requires i3d file in directory hierarchy):");
    eprintln!("  grleconvert input.png              → input.gdm or input.grle");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --i3d <path>        Specify i3d file path for encoding");
    eprintln!("  --channels <n>      Manual channel count (when no i3d)");
    eprintln!("  --compress-at <n>   Manual compression split (for GDM)");
    eprintln!();
    eprintln!("The tool auto-discovers the map .i3d file by walking up the");
    eprintln!("directory hierarchy from the input file location.");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    // Parse arguments
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut i3d_path: Option<String> = None;
    let mut manual_channels: Option<usize> = None;
    let mut manual_compress_at: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--i3d" => {
                i += 1;
                if i < args.len() {
                    i3d_path = Some(args[i].clone());
                }
            }
            "--channels" => {
                i += 1;
                if i < args.len() {
                    manual_channels = args[i].parse().ok();
                }
            }
            "--compress-at" => {
                i += 1;
                if i < args.len() {
                    manual_compress_at = args[i].parse().ok();
                }
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            arg if !arg.starts_with('-') => {
                if input_path.is_none() {
                    input_path = Some(arg.to_string());
                } else if output_path.is_none() {
                    output_path = Some(arg.to_string());
                }
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let input_path = match input_path {
        Some(p) => p,
        None => {
            print_usage();
            std::process::exit(1);
        }
    };

    let input_ext = Path::new(&input_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let result = match input_ext.as_str() {
        "grle" => {
            // Decode GRLE to PNG
            let output = output_path.unwrap_or_else(|| {
                let stem = Path::new(&input_path).file_stem().unwrap().to_str().unwrap();
                format!("{}.png", stem)
            });
            convert_grle_to_png(&input_path, &output)
        }
        "gdm" => {
            // Decode GDM to PNG
            let output = output_path.unwrap_or_else(|| {
                let stem = Path::new(&input_path).file_stem().unwrap().to_str().unwrap();
                format!("{}.png", stem)
            });
            convert_gdm_to_png(&input_path, &output)
        }
        "png" => {
            // Encode PNG to GRLE or GDM
            let input_abs = std::fs::canonicalize(&input_path).unwrap_or_else(|_| PathBuf::from(&input_path));
            let filename = input_abs.file_name().and_then(|f| f.to_str()).unwrap_or(&input_path);

            // Try to find i3d and discover parameters
            let i3d_file = if let Some(ref path) = i3d_path {
                eprintln!("Using specified i3d: {}", path);
                Some(PathBuf::from(path))
            } else {
                eprintln!("Searching for i3d file...");
                let found = find_i3d_file(&input_abs);
                if let Some(ref p) = found {
                    eprintln!("Found i3d: {}", p.display());
                }
                found
            };

            let params = if let Some(ref i3d) = i3d_file {
                parse_i3d_for_file(i3d, filename)
            } else {
                None
            };

            // Check if output extension explicitly specifies format
            let explicit_grle = output_path.as_ref().map(|p| {
                Path::new(p)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase() == "grle")
                    .unwrap_or(false)
            }).unwrap_or(false);

            // Determine parameters
            let params = match params {
                Some(p) => p,
                None => {
                    // Fall back to manual parameters or defaults
                    if let Some(channels) = manual_channels {
                        eprintln!("Using manual parameters: {} channels", channels);
                        let layer_type = if filename.contains("infoLayer") || explicit_grle {
                            LayerType::InfoLayer
                        } else {
                            LayerType::GdmLayer
                        };
                        LayerParams {
                            layer_type,
                            num_channels: channels,
                            compression_channels: manual_compress_at,
                        }
                    } else if explicit_grle {
                        // GRLE output explicitly requested - use default params
                        // GRLE doesn't need channel count for encoding
                        eprintln!("GRLE output requested, using default parameters");
                        LayerParams {
                            layer_type: LayerType::InfoLayer,
                            num_channels: 1,
                            compression_channels: None,
                        }
                    } else {
                        eprintln!("Error: Could not find i3d file or determine encoding parameters.");
                        eprintln!("Please specify --i3d <path> or --channels <n>");
                        eprintln!("Or specify output path with .grle extension for GRLE format.");
                        std::process::exit(1);
                    }
                }
            };

            // Determine output path and format
            let (output, use_grle) = if let Some(ref out_path) = output_path {
                let ext = Path::new(out_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_grle = ext == "grle";
                (out_path.clone(), is_grle)
            } else {
                let stem = Path::new(&input_path).file_stem().unwrap().to_str().unwrap();
                match params.layer_type {
                    LayerType::InfoLayer => (format!("{}.grle", stem), true),
                    LayerType::GdmLayer => (format!("{}.gdm", stem), false),
                }
            };

            eprintln!("Output: {}", output);

            if use_grle {
                convert_png_to_grle(&input_path, &output, &params)
            } else {
                convert_png_to_gdm(&input_path, &output, &params)
            }
        }
        _ => {
            eprintln!("Unknown file extension: {}", input_ext);
            eprintln!("Supported: .grle, .gdm, .png");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
