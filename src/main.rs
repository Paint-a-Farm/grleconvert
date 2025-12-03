use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read};
use std::path::Path;

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn decode_grle_rle(data: &[u8], expected_size: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(expected_size);
    let mut i = 1; // Skip first byte (0x00 flag/padding)

    // RLE format: read pairs (prev, new)
    // - If prev == new: read extended count, emit count pixels of that value
    //   - Count: each 0xff byte adds 255, final non-0xff byte is remainder, +2 offset
    // - If prev != new: emit 1 pixel of prev, then back up to re-read new as next prev
    while i + 1 < data.len() && output.len() < expected_size {
        let prev = data[i];
        let new_val = data[i + 1];
        i += 2;

        if prev == new_val {
            // Same value: read extended count with 0xff
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

            // Emit pixels
            let to_emit = count.min(expected_size - output.len());
            output.extend(std::iter::repeat(prev).take(to_emit));
        } else {
            // Transition: emit 1 pixel of prev, back up to re-read new as next prev
            output.push(prev);
            i -= 1; // Back up so new_val becomes next prev
        }
    }

    // Pad with zeros if needed (shouldn't happen with valid files)
    output.resize(expected_size, 0);
    output
}

fn convert_grle(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufReader::new(File::open(input_path)?);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Check magic
    if &data[0..4] != b"GRLE" {
        return Err("Not a valid GRLE file".into());
    }

    let version = read_u16_le(&data, 4);
    // Width and height stored as size/256 in u16 with 2 byte padding between
    let width = (read_u16_le(&data, 6) as usize) * 256;
    let height = (read_u16_le(&data, 10) as usize) * 256;
    // GRLE files always have 1 channel (grayscale)
    let channels = 1usize;
    let _compressed_size = read_u32_le(&data, 16);

    eprintln!("GRLE version: {}", version);
    eprintln!("Size: {}x{}", width, height);
    eprintln!("Channels: {}", channels);

    // Data starts at offset 20
    let compressed_data = &data[20..];
    let expected_size = width * height * channels;

    let pixels = decode_grle_rle(compressed_data, expected_size);

    // Write PNG
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

// GDM block decoder - decodes a single compressed block
fn decode_gdm_block(data: &[u8], pos: usize, chunk_size: usize) -> (Vec<u16>, usize) {
    let bit_depth = data[pos];
    let palette_count = data[pos + 1] as usize;
    let palette_size = 2 * palette_count;
    let bitmap_size = if bit_depth > 0 { (bit_depth as usize) * 128 } else { 0 };
    let block_size = 2 + palette_size + bitmap_size;

    // Read palette
    let palette: Vec<u16> = (0..palette_count)
        .map(|i| u16::from_le_bytes([data[pos + 2 + i*2], data[pos + 3 + i*2]]))
        .collect();

    let total_pixels = chunk_size * chunk_size;
    let mut pixels = Vec::with_capacity(total_pixels);

    if bit_depth == 0 {
        // Uniform chunk - all pixels have the same value
        let value = *palette.first().unwrap_or(&0);
        pixels.resize(total_pixels, value);
    } else {
        // Decode bitmap
        let bitmap = &data[pos + 2 + palette_size..pos + 2 + palette_size + bitmap_size];
        let bits_per_pixel = bit_depth as usize;
        let mask = (1u16 << bits_per_pixel) - 1;

        for pixel_idx in 0..total_pixels {
            let bit_pos = pixel_idx * bits_per_pixel;
            let byte_idx = bit_pos / 8;
            let bit_offset = bit_pos % 8;

            // Read up to 2 bytes for the value
            let mut raw_value = bitmap[byte_idx] as u16;
            if byte_idx + 1 < bitmap.len() {
                raw_value |= (bitmap[byte_idx + 1] as u16) << 8;
            }

            let idx_or_value = ((raw_value >> bit_offset) & mask) as usize;

            // For bit_depth > 2, the bitmap contains raw values, not palette indices
            // For bit_depth <= 2, the bitmap contains palette indices
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

fn convert_gdm(input_path: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufReader::new(File::open(input_path)?);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Check magic - "MDF or !MDF
    if data.len() < 16 {
        return Err("File too small".into());
    }

    let magic = &data[0..4];
    if magic != b"\"MDF" && magic != b"!MDF" {
        return Err("Not a valid GDM file".into());
    }

    // Parse header based on variant
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
            // !MDF (0x21) variant
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

    // Read compression boundaries to determine channel ranges
    let mut compression_boundaries = vec![0u8];
    for i in 0..(num_compression_ranges.saturating_sub(1)) {
        compression_boundaries.push(data[header_size + i]);
    }
    compression_boundaries.push(num_channels as u8);

    // Calculate bits per compression range
    let mut bits_per_range = Vec::new();
    for i in 0..num_compression_ranges {
        let start_ch = compression_boundaries[i] as usize;
        let end_ch = compression_boundaries[i + 1] as usize;
        bits_per_range.push(end_ch - start_ch);
    }

    let chunks_per_dim = dimension / chunk_size;
    let total_chunks = chunks_per_dim * chunks_per_dim;

    // Calculate data start position (after compression boundaries)
    let compression_boundaries_size = if num_compression_ranges > 1 { num_compression_ranges - 1 } else { 0 };
    let data_start = header_size + compression_boundaries_size;

    // Determine output format
    let use_rgb = num_channels > 8;

    // Create output image
    let bytes_per_pixel = if use_rgb { 3 } else { 1 };
    let mut image = vec![0u8; dimension * dimension * bytes_per_pixel];

    // Process each chunk sequentially
    let mut pos = data_start;

    for chunk_idx in 0..total_chunks {
        // Decode all compression ranges for this chunk
        let mut range_values: Vec<Vec<u16>> = Vec::new();

        for _range_idx in 0..num_compression_ranges {
            if pos + 2 > data.len() {
                return Err("Unexpected end of data".into());
            }

            let (pixels, block_size) = decode_gdm_block(&data, pos, chunk_size);
            range_values.push(pixels);
            pos += block_size;
        }

        // Calculate chunk position
        let chunk_row = chunk_idx / chunks_per_dim;
        let chunk_col = chunk_idx % chunks_per_dim;
        let base_y = chunk_row * chunk_size;
        let base_x = chunk_col * chunk_size;

        // Combine range values and write to image
        for pixel_idx in 0..(chunk_size * chunk_size) {
            // Combine values from all ranges into a single value
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
                // RGB: R = bits 0-7, G = bits 8-15, B = bits 16-23
                let r = (combined & 0xFF) as u8;
                let g = ((combined >> 8) & 0xFF) as u8;
                let b = ((combined >> 16) & 0xFF) as u8;
                let img_idx = (img_y * dimension + img_x) * 3;
                image[img_idx] = r;
                image[img_idx + 1] = g;
                image[img_idx + 2] = b;
            } else {
                // Grayscale: just use the low 8 bits
                let img_idx = img_y * dimension + img_x;
                image[img_idx] = (combined & 0xFF) as u8;
            }
        }
    }

    eprintln!("Data consumed: {} / {} bytes", pos, data.len());

    // Write PNG
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

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: grleconvert <input.grle> [output.png]");
        eprintln!("       grleconvert <input.gdm> [output.png]");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() > 2 {
        args[2].clone()
    } else {
        let path = Path::new(input_path);
        let stem = path.file_stem().unwrap().to_str().unwrap();
        format!("{}.png", stem)
    };

    let ext = Path::new(input_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let result = match ext.as_str() {
        "grle" => convert_grle(input_path, &output_path),
        "gdm" => convert_gdm(input_path, &output_path),
        _ => {
            eprintln!("Unknown file extension: {}", ext);
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
