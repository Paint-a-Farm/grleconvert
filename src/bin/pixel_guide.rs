/// Generates a pixel value translation guide for GDM and GRLE files
/// by parsing the map's i3d file and related XML configuration files.
///
/// Usage: pixel_guide <map.i3d> [output.md] [--data-dir <path>]
///
/// Parses map-specific config files (referenced in maps.xml) with fallback
/// to base game files when --data-dir is provided.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Configuration data loaded from map XML files
#[derive(Default, Debug)]
struct MapConfig {
    /// Fill types for densityMap_height (index -> name)
    fill_types: Vec<(u32, String)>,
    /// Ground types for densityMap_ground (value -> name)
    ground_types: Vec<(u32, String)>,
    /// Spray types for densityMap_ground (value -> name)
    spray_types: Vec<(u32, String)>,
    /// Farmland IDs (id -> description)
    farmlands: Vec<(u32, String)>,
    /// Fruit types for densityMap_fruits (index -> name)
    fruit_types: Vec<(u32, String)>,
    /// Weed blocking state value (from weed.xml)
    weed_blocking_value: Option<u32>,
    /// Whether configs were loaded successfully
    has_fill_types: bool,
    has_ground_types: bool,
    has_farmlands: bool,
    has_fruit_types: bool,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <map.i3d> [output.md] [--data-dir <path>]", args[0]);
        eprintln!();
        eprintln!("Generates a pixel value translation guide for GDM and GRLE files");
        eprintln!("by parsing the map's i3d file.");
        eprintln!();
        eprintln!("The map's i3d file alone contains:");
        eprintln!("  - All InfoLayer definitions (GRLE files)");
        eprintln!("  - Ground/height DetailLayer definitions (GDM files)");
        eprintln!("  - Foliage type listings (with default state descriptions)");
        eprintln!();
        eprintln!("For detailed foliage state info, provide --data-dir to the base game data folder.");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  {} mapUS.i3d", args[0]);
        eprintln!("  {} mapUS.i3d pixel_guide.md", args[0]);
        eprintln!("  {} mapUS.i3d pixel_guide.md --data-dir /path/to/data", args[0]);
        std::process::exit(1);
    }

    let i3d_path = &args[1];

    // Parse optional arguments
    let mut output_path: Option<&str> = None;
    let mut data_dir: Option<&str> = None;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--data-dir" && i + 1 < args.len() {
            data_dir = Some(&args[i + 1]);
            i += 2;
        } else if output_path.is_none() && !args[i].starts_with("--") {
            output_path = Some(&args[i]);
            i += 1;
        } else {
            i += 1;
        }
    }

    match generate_guide(i3d_path, output_path, data_dir) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn generate_guide(i3d_path: &str, output_path: Option<&str>, data_dir: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(i3d_path)?;
    let map_name = Path::new(i3d_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown");

    // Load all map configuration from XML files
    let map_config = load_map_config(i3d_path, data_dir);

    let mut output = String::new();

    output.push_str(&format!("# Pixel Color Guide for {}\n\n", map_name));
    output.push_str("This document shows what RGB/Gray color to paint for each value in the density map and info layer PNG files.\n\n");
    output.push_str("## Table of Contents\n\n");

    // Parse and collect all layer info
    let mut sections = Vec::new();

    // Parse InfoLayers (GRLE files) - pass config for farmlands
    sections.extend(parse_info_layers(&content, &map_config));

    // Parse DetailLayers (GDM files) - pass config for fill types and ground types
    sections.extend(parse_detail_layers(&content, &map_config));

    // Parse FoliageMultiLayers (GDM files)
    sections.extend(parse_foliage_layers(&content, i3d_path, data_dir));

    // Generate TOC
    for section in &sections {
        output.push_str(&format!("- [{}](#{})\n", section.name, section.name.to_lowercase().replace(' ', "-").replace(['(', ')'], "")));
    }
    output.push_str("\n---\n\n");

    // Generate content
    for section in &sections {
        output.push_str(&format!("## {}\n\n", section.name));
        output.push_str(&format!("**File:** `{}`\n\n", section.filename));

        // Determine color mode
        let is_rgb = section.num_channels > 8;
        let color_mode = if is_rgb { "RGB" } else { "Grayscale" };
        output.push_str(&format!("**Color Mode:** {} ({} channels)\n\n", color_mode, section.num_channels));

        if !section.description.is_empty() {
            output.push_str(&format!("{}\n\n", section.description));
        }

        // Special handling for height layer
        if section.name.contains("Height") && section.name.contains("terrainDetailHeight") {
            generate_height_layer_table(&mut output, &section);
        }
        // For single-group layers, show direct RGB values
        else if section.groups.len() == 1 {
            let group = &section.groups[0];
            output.push_str(&format!("### {}\n\n", if group.name.is_empty() { "Values".to_string() } else { group.name.clone() }));

            if is_rgb {
                output.push_str("| RGB | Hex | Meaning |\n");
                output.push_str("|-----|-----|--------|\n");
                for (value, name) in &group.options {
                    let (r, g, b) = value_to_rgb(*value, section.num_channels);
                    output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} |\n", r, g, b, r, g, b, name));
                }
            } else {
                output.push_str("| Gray | Hex | Meaning |\n");
                output.push_str("|------|-----|--------|\n");
                for (value, name) in &group.options {
                    output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
                }
            }
            output.push_str("\n");
        } else if !section.groups.is_empty() {
            // Multi-group layers - generate all practical combinations
            generate_multi_group_table(&mut output, &section, is_rgb);
        }

        output.push_str("---\n\n");
    }

    // Add brief format notes
    output.push_str("## Color Format Notes\n\n");
    output.push_str("- **Grayscale files**: Paint with the gray value shown (R=G=B=value)\n");
    output.push_str("- **RGB files**: Paint with the exact RGB values shown\n");
    output.push_str("- For layers with multiple attributes, find your combination in the table above\n");

    // Output
    if let Some(path) = output_path {
        let mut file = fs::File::create(path)?;
        file.write_all(output.as_bytes())?;
        eprintln!("Guide written to: {}", path);
    } else {
        print!("{}", output);
    }

    Ok(())
}

/// Generate a table of all practical combinations for multi-group layers
fn generate_multi_group_table(output: &mut String, section: &LayerSection, is_rgb: bool) {
    // Special handling for known layer types
    if section.name.contains("Ground") && !section.name.contains("Foliage") {
        generate_ground_table(output, section, is_rgb);
    } else if section.name.contains("Fruits") || section.name.contains("Foliage") && section.groups.len() == 2 {
        generate_fruits_table(output, section, is_rgb);
    } else if section.name.contains("Environment") {
        generate_environment_table(output, section);
    } else {
        // Generic multi-group - show each group separately with pre-computed values
        for group in &section.groups {
            output.push_str(&format!("### {}\n\n", group.name));
            if is_rgb {
                output.push_str("| RGB | Hex | Meaning |\n");
                output.push_str("|-----|-----|--------|\n");
                for (value, name) in &group.options {
                    let shifted = value << group.first_channel;
                    let (r, g, b) = value_to_rgb(shifted, section.num_channels);
                    output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} |\n", r, g, b, r, g, b, name));
                }
            } else {
                output.push_str("| Gray | Hex | Meaning |\n");
                output.push_str("|------|-----|--------|\n");
                for (value, name) in &group.options {
                    let shifted = value << group.first_channel;
                    output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", shifted, shifted, name));
                }
            }
            output.push_str("\n");
        }
    }
}

/// Generate ground layer table with all GroundType + SprayType combinations
fn generate_ground_table(output: &mut String, section: &LayerSection, is_rgb: bool) {
    // Find the groups
    let ground_group = section.groups.iter().find(|g| g.name.contains("GroundType") || g.name.contains("Type") && g.first_channel == 0);
    let spray_group = section.groups.iter().find(|g| g.name.contains("Spray"));
    let water_group = section.groups.iter().find(|g| g.name.contains("Water"));

    // Ground types (most commonly needed)
    output.push_str("### Ground Types (base colors)\n\n");
    output.push_str("These are the base colors for each ground type. Add spray/water values to these.\n\n");

    if let Some(ground) = ground_group {
        if is_rgb {
            output.push_str("| RGB | Hex | Ground Type |\n");
            output.push_str("|-----|-----|-------------|\n");
        } else {
            output.push_str("| Gray | Hex | Ground Type |\n");
            output.push_str("|------|-----|-------------|\n");
        }
        for (value, name) in &ground.options {
            let (r, g, b) = value_to_rgb(*value, section.num_channels);
            if is_rgb {
                output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} |\n", r, g, b, r, g, b, name));
            } else {
                output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
            }
        }
        output.push_str("\n");
    }

    // Spray type additions
    if let Some(spray) = spray_group {
        output.push_str("### Spray Types (add to ground type)\n\n");
        output.push_str("Add these RGB values to the ground type color.\n\n");

        if is_rgb {
            output.push_str("| R | G | B | Spray Type |\n");
            output.push_str("|---|---|---|------------|\n");
            for (value, name) in &spray.options {
                let shifted = value << spray.first_channel;
                let (r, g, b) = value_to_rgb(shifted, section.num_channels);
                output.push_str(&format!("| `+{}` | `+{}` | `+{}` | {} |\n", r, g, b, name));
            }
        } else {
            output.push_str("| Add to Gray | Spray Type |\n");
            output.push_str("|-------------|------------|\n");
            for (value, name) in &spray.options {
                let add_value = value << spray.first_channel;
                output.push_str(&format!("| `+{}` | {} |\n", add_value, name));
            }
        }
        output.push_str("\n");
    }

    // Water flag
    if let Some(water) = water_group {
        output.push_str("### Watered Flag (add to ground type)\n\n");
        for (value, name) in &water.options {
            if *value > 0 {
                let shifted = value << water.first_channel;
                let (r, g, b) = value_to_rgb(shifted, section.num_channels);
                if is_rgb {
                    output.push_str(&format!("Add R=`+{}`, G=`+{}`, B=`+{}` for: {}\n\n", r, g, b, name));
                } else {
                    output.push_str(&format!("Add `+{}` for: {}\n\n", shifted, name));
                }
            }
        }
    }

    // Common complete examples
    output.push_str("### Common Complete Colors\n\n");
    if is_rgb {
        output.push_str("| RGB | Hex | Description |\n");
        output.push_str("|-----|-----|-------------|\n");
    } else {
        output.push_str("| Gray | Hex | Description |\n");
        output.push_str("|------|-----|-------------|\n");
    }

    let examples = [
        (0u32, "Empty/Natural ground"),
        (7, "Sown field"),
        (7 + 128, "Sown + Fertilized"),
        (7 + 256, "Sown + Manure"),
        (7 + 384, "Sown + Slurry"),
        (7 + 512, "Sown + Lime"),
        (4, "Plowed"),
        (4 + 128, "Plowed + Fertilized"),
        (2, "Cultivated"),
        (3, "Seedbed"),
        (14, "Grass"),
        (15, "Grass (cut)"),
        (12, "Harvest-ready"),
    ];

    for (val, desc) in examples {
        let (r, g, b) = value_to_rgb(val, section.num_channels);
        if is_rgb {
            output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} |\n", r, g, b, r, g, b, desc));
        } else {
            output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", val, val, desc));
        }
    }
    output.push_str("\n");
}

/// Generate fruits/foliage table with type + state combinations
fn generate_fruits_table(output: &mut String, section: &LayerSection, is_rgb: bool) {
    let type_group = section.groups.iter().find(|g| g.name.contains("Type"));
    let state_group = section.groups.iter().find(|g| g.name.contains("State") || g.name.contains("Growth"));

    if let (Some(types), Some(states)) = (type_group, state_group) {
        // Show type index colors
        output.push_str("### Crop/Foliage Types (base colors)\n\n");
        if is_rgb {
            output.push_str("| RGB | Hex | Type |\n");
            output.push_str("|-----|-----|------|\n");
        } else {
            output.push_str("| Gray | Hex | Type |\n");
            output.push_str("|------|-----|------|\n");
        }
        for (value, name) in &types.options {
            let (r, g, b) = value_to_rgb(*value, section.num_channels);
            if is_rgb {
                output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} |\n", r, g, b, r, g, b, name));
            } else {
                output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
            }
        }
        output.push_str("\n");

        // Show growth states with their additions
        output.push_str("### Growth States (add to type)\n\n");
        output.push_str("Add these RGB values to the crop type color.\n\n");

        if is_rgb {
            output.push_str("| R | G | B | Growth State |\n");
            output.push_str("|---|---|---|-------------|\n");
            for (value, name) in &states.options {
                let shifted = value << states.first_channel;
                let (r, g, b) = value_to_rgb(shifted, section.num_channels);
                output.push_str(&format!("| `+{}` | `+{}` | `+{}` | {} |\n", r, g, b, name));
            }
        } else {
            output.push_str("| Add to Gray | Growth State |\n");
            output.push_str("|-------------|-------------|\n");
            for (value, name) in &states.options {
                let add_value = value << states.first_channel;
                output.push_str(&format!("| `+{}` | {} |\n", add_value, name));
            }
        }
        output.push_str("\n");

        // Common examples
        output.push_str("### Common Complete Colors\n\n");
        if is_rgb {
            output.push_str("| RGB | Hex | Description |\n");
            output.push_str("|-----|-----|-------------|\n");
        } else {
            output.push_str("| Gray | Hex | Description |\n");
            output.push_str("|------|-----|-------------|\n");
        }

        // Generate examples for common crops
        let common_crops = [
            (6, "Wheat"), (7, "Canola"), (8, "Barley"), (9, "Maize"), (5, "Grass"),
        ];
        let common_states = [
            (4 << states.first_channel, "harvest-ready"),
            (2 << states.first_channel, "small"),
        ];

        for (crop_val, crop_name) in common_crops {
            for (state_add, state_name) in &common_states {
                let val = crop_val + state_add;
                let (r, g, b) = value_to_rgb(val, section.num_channels);
                if is_rgb {
                    output.push_str(&format!("| `{}, {}, {}` | `#{:02X}{:02X}{:02X}` | {} ({}) |\n",
                        r, g, b, r, g, b, crop_name, state_name));
                } else {
                    output.push_str(&format!("| `{}` | `#{:02X}` | {} ({}) |\n", val, val, crop_name, state_name));
                }
            }
        }
        output.push_str("\n");
    }
}

/// Generate height layer table with fill types in R and heights in G
fn generate_height_layer_table(output: &mut String, section: &LayerSection) {
    // Find fill type and height groups
    let fill_group = section.groups.iter().find(|g| g.name.contains("Fill Type"));
    let height_group = section.groups.iter().find(|g| g.name.contains("Height"));

    // Fill types - R channel
    if let Some(fill) = fill_group {
        output.push_str("### Fill Types (R channel)\n\n");
        output.push_str("Paint the R (red) channel with these values:\n\n");
        output.push_str("| R | Hex | Fill Type |\n");
        output.push_str("|---|-----|----------|\n");
        for (value, name) in &fill.options {
            output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
        }
        output.push_str("\n");
    }

    // Height examples - G channel
    if let Some(height) = height_group {
        output.push_str("### Height Values (G channel)\n\n");
        output.push_str("Paint the G (green) channel with these values:\n\n");
        output.push_str("| G | Hex | Height |\n");
        output.push_str("|---|-----|--------|\n");
        for (value, name) in &height.options {
            output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
        }
        output.push_str("\n");
    }

    // Combined examples
    output.push_str("### Example Complete Colors\n\n");
    output.push_str("| R | G | B | Hex | Description |\n");
    output.push_str("|---|---|---|-----|-------------|\n");

    let examples = [
        (0, 0, "Empty"),
        (1, 32, "Wheat pile ~2m"),
        (20, 63, "Straw pile ~4m"),
        (29, 16, "Stone pile ~1m"),
        (18, 48, "Grass pile ~3m"),
    ];

    for (r, g, desc) in examples {
        output.push_str(&format!("| `{}` | `{}` | `0` | `#{:02X}{:02X}00` | {} |\n", r, g, r, g, desc));
    }
    output.push_str("\n");
}

/// Generate environment layer table
fn generate_environment_table(output: &mut String, section: &LayerSection) {
    let area_group = section.groups.iter().find(|g| g.name.contains("Area") || g.name.contains("Type") && g.first_channel == 0);
    let water_group = section.groups.iter().find(|g| g.name.contains("Water"));

    // First show base area types
    if let Some(area) = area_group {
        output.push_str("### Area Types\n\n");
        output.push_str("| Gray | Hex | Area Type |\n");
        output.push_str("|------|-----|----------|\n");
        for (value, name) in &area.options {
            output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
        }
        output.push_str("\n");
    }

    // Water proximity flag
    if let Some(water) = water_group {
        if let Some((_, name)) = water.options.iter().find(|(v, _)| *v > 0) {
            let add_value = 1u32 << water.first_channel;
            output.push_str("### Water Proximity\n\n");
            output.push_str(&format!("Add `+{}` to any area type for: {}\n\n", add_value, name));
        }
    }

    // Full table with water combinations
    output.push_str("### Complete Values\n\n");
    output.push_str("| Gray | Hex | Description |\n");
    output.push_str("|------|-----|-------------|\n");

    if let Some(area) = area_group {
        for (value, name) in &area.options {
            output.push_str(&format!("| `{}` | `#{:02X}` | {} |\n", value, value, name));
            // With water
            if let Some(water) = water_group {
                if let Some((water_val, water_name)) = water.options.iter().find(|(v, _)| *v > 0) {
                    let with_water = value + (water_val << water.first_channel);
                    output.push_str(&format!("| `{}` | `#{:02X}` | {} + {} |\n", with_water, with_water, name, water_name));
                }
            }
        }
    }
    output.push_str("\n");
}

/// Convert a packed value to RGB components based on channel count
fn value_to_rgb(value: u32, num_channels: u32) -> (u8, u8, u8) {
    if num_channels <= 8 {
        // Grayscale - same value in all channels
        let v = (value & 0xFF) as u8;
        (v, v, v)
    } else {
        // RGB - split across channels
        let r = (value & 0xFF) as u8;
        let g = ((value >> 8) & 0xFF) as u8;
        let b = ((value >> 16) & 0xFF) as u8;
        (r, g, b)
    }
}

struct LayerSection {
    name: String,
    filename: String,
    file_type: String,
    num_channels: u32,
    description: String,
    groups: Vec<ChannelGroup>,
}

/// Load all map configuration from XML files
/// Priority for each config type:
/// 1. Map-specific file (from maps.xml reference)
/// 2. Base game file (from --data-dir if provided)
fn load_map_config(i3d_path: &str, data_dir: Option<&str>) -> MapConfig {
    let mut config = MapConfig::default();
    let i3d_dir = match Path::new(i3d_path).parent() {
        Some(d) => d,
        None => return config,
    };

    // Find maps.xml and mod root
    let (maps_xml_path, mod_root) = match find_maps_xml(i3d_dir) {
        Some(path) => {
            let root = path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
            (Some(path), root)
        }
        None => (None, None),
    };

    let maps_content = maps_xml_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok());

    // Load fill types (densityMapHeightTypes)
    config.fill_types = load_config_file(
        &maps_content,
        &mod_root,
        data_dir,
        "densityMapHeightTypes",
        "maps/maps_densityMapHeightTypes.xml",
        parse_fill_types,
    );
    config.has_fill_types = !config.fill_types.is_empty();

    // Load ground types and spray types (fieldGround)
    let (ground, spray) = load_field_ground_config(
        &maps_content,
        &mod_root,
        data_dir,
    );
    config.ground_types = ground;
    config.spray_types = spray;
    config.has_ground_types = !config.ground_types.is_empty();

    // Load farmlands
    config.farmlands = load_config_file(
        &maps_content,
        &mod_root,
        data_dir,
        "farmlands",
        "", // No base game fallback for farmlands
        parse_farmlands,
    );
    config.has_farmlands = !config.farmlands.is_empty();

    // Load weed config
    config.weed_blocking_value = load_weed_config(&maps_content, &mod_root, data_dir);

    // Load fruit types
    config.fruit_types = load_fruit_types(&maps_content, &mod_root, data_dir);
    config.has_fruit_types = !config.fruit_types.is_empty();

    config
}

/// Resolve a path that may contain $data/ prefix
fn resolve_path(filename: &str, mod_root: &Option<PathBuf>, data_dir: Option<&str>) -> Option<PathBuf> {
    if filename.starts_with("$data/") {
        // Path relative to game data folder
        if let Some(data_path) = data_dir {
            let relative = filename.trim_start_matches("$data/");
            return Some(Path::new(data_path).join(relative));
        }
        None
    } else {
        // Path relative to mod root
        mod_root.as_ref().map(|root| root.join(filename))
    }
}

/// Generic config file loader with map-specific and base game fallback
fn load_config_file<F>(
    maps_content: &Option<String>,
    mod_root: &Option<PathBuf>,
    data_dir: Option<&str>,
    element_name: &str,
    base_game_path: &str,
    parser: F,
) -> Vec<(u32, String)>
where
    F: Fn(&str) -> Vec<(u32, String)>,
{
    // Try map-specific file first
    if let Some(content) = maps_content {
        if let Some(filename) = find_config_filename(content, element_name) {
            if let Some(config_path) = resolve_path(&filename, mod_root, data_dir) {
                if let Ok(config_content) = fs::read_to_string(&config_path) {
                    let result = parser(&config_content);
                    if !result.is_empty() {
                        eprintln!("Loaded {} from {}", element_name, config_path.display());
                        return result;
                    }
                }
            }
        }
    }

    // Fall back to base game file
    if !base_game_path.is_empty() {
        if let Some(data_path) = data_dir {
            let path = Path::new(data_path).join(base_game_path);
            if let Ok(content) = fs::read_to_string(&path) {
                let result = parser(&content);
                if !result.is_empty() {
                    eprintln!("Loaded {} from {}", element_name, path.display());
                    return result;
                }
            }
        }
    }

    Vec::new()
}

/// Find config filename in maps.xml for a given element
fn find_config_filename(maps_content: &str, element_name: &str) -> Option<String> {
    let search = format!("<{}", element_name);
    for line in maps_content.lines() {
        if line.contains(&search) && line.contains("filename=") {
            return extract_attr(line, "filename");
        }
    }
    None
}

/// Load fieldGround.xml for ground types and spray types
fn load_field_ground_config(
    maps_content: &Option<String>,
    mod_root: &Option<PathBuf>,
    data_dir: Option<&str>,
) -> (Vec<(u32, String)>, Vec<(u32, String)>) {
    let parse_both = |content: &str| -> (Vec<(u32, String)>, Vec<(u32, String)>) {
        (parse_ground_types(content), parse_spray_types(content))
    };

    // Try map-specific file first
    if let Some(content) = maps_content {
        if let Some(filename) = find_config_filename(content, "fieldGround") {
            if let Some(config_path) = resolve_path(&filename, mod_root, data_dir) {
                if let Ok(config_content) = fs::read_to_string(&config_path) {
                    let (ground, spray) = parse_both(&config_content);
                    if !ground.is_empty() {
                        eprintln!("Loaded fieldGround from {}", config_path.display());
                        return (ground, spray);
                    }
                }
            }
        }
    }

    // Fall back to base game file
    if let Some(data_path) = data_dir {
        let path = Path::new(data_path).join("maps/maps_fieldGround.xml");
        if let Ok(content) = fs::read_to_string(&path) {
            let (ground, spray) = parse_both(&content);
            if !ground.is_empty() {
                eprintln!("Loaded fieldGround from {}", path.display());
                return (ground, spray);
            }
        }
    }

    (Vec::new(), Vec::new())
}

/// Parse densityMapHeightTypes.xml and extract fill type names
fn parse_fill_types(content: &str) -> Vec<(u32, String)> {
    let mut fill_types = vec![(0u32, "Empty".to_string())];
    let mut index = 1u32;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("<densityMapHeightType ") || line.contains("<densityMapHeightType ") {
            if let Some(name) = extract_attr(line, "fillTypeName") {
                fill_types.push((index, titlecase_name(&name)));
                index += 1;
            }
        }
    }

    if fill_types.len() > 1 { fill_types } else { Vec::new() }
}

/// Parse fieldGround.xml groundTypes section
fn parse_ground_types(content: &str) -> Vec<(u32, String)> {
    let mut types = vec![(0u32, "Natural".to_string())];
    let mut in_ground_types = false;

    for line in content.lines() {
        let line = line.trim();
        if line.contains("<groundTypes") {
            in_ground_types = true;
        } else if line.contains("</groundTypes") {
            in_ground_types = false;
        } else if in_ground_types && line.starts_with("<") && line.contains("value=") {
            // Extract element name (e.g., <stubbleTillage value="1"...)
            if let Some(end) = line.find(' ') {
                let name = &line[1..end];
                if let Some(value) = extract_attr(line, "value") {
                    if let Ok(val) = value.parse::<u32>() {
                        types.push((val, titlecase_name(name)));
                    }
                }
            }
        }
    }

    if types.len() > 1 { types } else { Vec::new() }
}

/// Parse fieldGround.xml sprayTypes section
fn parse_spray_types(content: &str) -> Vec<(u32, String)> {
    let mut types = vec![(0u32, "None".to_string())];
    let mut in_spray_types = false;

    for line in content.lines() {
        let line = line.trim();
        if line.contains("<sprayTypes") {
            in_spray_types = true;
        } else if line.contains("</sprayTypes") {
            in_spray_types = false;
        } else if in_spray_types && line.starts_with("<") && line.contains("value=") {
            if let Some(end) = line.find(' ') {
                let name = &line[1..end];
                if let Some(value) = extract_attr(line, "value") {
                    if let Ok(val) = value.parse::<u32>() {
                        types.push((val, titlecase_name(name)));
                    }
                }
            }
        }
    }

    if types.len() > 1 { types } else { Vec::new() }
}

/// Parse farmlands.xml and extract farmland IDs
fn parse_farmlands(content: &str) -> Vec<(u32, String)> {
    let mut farmlands = vec![(0u32, "Not owned".to_string())];

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("<farmland ") || line.contains("<farmland ") {
            if let Some(id) = extract_attr(line, "id") {
                if let Ok(id_val) = id.parse::<u32>() {
                    let desc = if extract_attr(line, "defaultFarmProperty").as_deref() == Some("true") {
                        format!("Farmland {} (starting)", id_val)
                    } else {
                        format!("Farmland {}", id_val)
                    };
                    farmlands.push((id_val, desc));
                }
            }
        }
    }

    // Sort by ID
    farmlands.sort_by_key(|(id, _)| *id);
    if farmlands.len() > 1 { farmlands } else { Vec::new() }
}

/// Load weed.xml config for blocking state value
fn load_weed_config(
    maps_content: &Option<String>,
    mod_root: &Option<PathBuf>,
    data_dir: Option<&str>,
) -> Option<u32> {
    // Try map-specific file first
    if let Some(content) = maps_content {
        if let Some(filename) = find_config_filename(content, "weed") {
            if let Some(config_path) = resolve_path(&filename, mod_root, data_dir) {
                if let Ok(config_content) = fs::read_to_string(&config_path) {
                    if let Some(value) = parse_weed_blocking_value(&config_content) {
                        eprintln!("Loaded weed config from {}", config_path.display());
                        return Some(value);
                    }
                }
            }
        }
    }

    // Fall back to base game file
    if let Some(data_path) = data_dir {
        let path = Path::new(data_path).join("maps/maps_weed.xml");
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(value) = parse_weed_blocking_value(&content) {
                eprintln!("Loaded weed config from {}", path.display());
                return Some(value);
            }
        }
    }

    None
}

/// Parse weed.xml and extract blocking state value
fn parse_weed_blocking_value(content: &str) -> Option<u32> {
    for line in content.lines() {
        let line = line.trim();
        if line.contains("<blockingState") && line.contains("value=") {
            return extract_attr(line, "value").and_then(|v| v.parse().ok());
        }
    }
    None
}

/// Load fruit types from fruitTypes.xml or maps.xml
fn load_fruit_types(
    maps_content: &Option<String>,
    mod_root: &Option<PathBuf>,
    data_dir: Option<&str>,
) -> Vec<(u32, String)> {
    let mut fruit_types = Vec::new();

    // Helper to parse fruit types from a fruitTypes XML file
    let parse_fruit_types_file = |content: &str, mod_root: &Option<PathBuf>, data_dir: Option<&str>| -> Vec<(u32, String)> {
        let mut fruits = Vec::new();
        let mut idx = 1u32;

        for line in content.lines() {
            let line = line.trim();
            // Look for <fruitType filename="..." /> entries
            if line.contains("<fruitType") && line.contains("filename=") {
                if let Some(filename) = extract_attr(line, "filename") {
                    // Try to load the fruit XML to get the name
                    if let Some(fruit_path) = resolve_path(&filename, mod_root, data_dir) {
                        if let Ok(fruit_content) = fs::read_to_string(&fruit_path) {
                            // Look for <fruitType name="..."> in the fruit XML
                            for fruit_line in fruit_content.lines() {
                                if fruit_line.contains("<fruitType") && fruit_line.contains("name=") {
                                    if let Some(name) = extract_attr(fruit_line, "name") {
                                        fruits.push((idx, titlecase_name(&name)));
                                        break;
                                    }
                                }
                            }
                        } else {
                            // Can't read file, extract name from filename
                            let name = Path::new(&filename)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Unknown");
                            fruits.push((idx, titlecase_name(name)));
                        }
                    } else {
                        // Can't resolve path, extract name from filename
                        let name = Path::new(&filename)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unknown");
                        fruits.push((idx, titlecase_name(name)));
                    }
                    idx += 1;
                }
            }
        }
        fruits
    };

    // First, try to load from map's fruitTypes.xml config file
    if let Some(content) = maps_content {
        if let Some(filename) = find_config_filename(content, "fruitTypes") {
            if let Some(config_path) = resolve_path(&filename, mod_root, data_dir) {
                if let Ok(config_content) = fs::read_to_string(&config_path) {
                    fruit_types = parse_fruit_types_file(&config_content, mod_root, data_dir);
                    if !fruit_types.is_empty() {
                        eprintln!("Loaded fruitTypes from {}", config_path.display());
                        return fruit_types;
                    }
                }
            }
        }
    }

    // Fall back to base game fruitTypes
    if let Some(data_path) = data_dir {
        let path = Path::new(data_path).join("maps/maps_fruitTypes.xml");
        if let Ok(content) = fs::read_to_string(&path) {
            fruit_types = parse_fruit_types_file(&content, mod_root, data_dir);
            if !fruit_types.is_empty() {
                eprintln!("Loaded fruitTypes from {}", path.display());
                return fruit_types;
            }
        }
    }

    fruit_types
}

/// Convert UPPERCASE_NAME or camelCase to Title Case
fn titlecase_name(name: &str) -> String {
    // Handle UPPERCASE_NAMES
    if name.contains('_') {
        return name
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect::<String>()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }

    // Handle camelCase
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if i == 0 {
            result.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            result.push(' ');
            result.extend(c.to_uppercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Find maps.xml in the directory or its parent
fn find_maps_xml(dir: &Path) -> Option<PathBuf> {
    // Helper to find mod/DLC root (directory containing modDesc.xml or dlcDesc.xml)
    let find_mod_root = |start: &Path| -> Option<(PathBuf, PathBuf)> {
        let mut current = Some(start.to_path_buf());
        while let Some(d) = current {
            let mod_desc = d.join("modDesc.xml");
            if mod_desc.exists() {
                return Some((d, mod_desc));
            }
            let dlc_desc = d.join("dlcDesc.xml");
            if dlc_desc.exists() {
                return Some((d, dlc_desc));
            }
            current = d.parent().map(|p| p.to_path_buf());
        }
        None
    };

    // First, check modDesc.xml or dlcDesc.xml for authoritative configFilename
    if let Some((mod_root, desc_path)) = find_mod_root(dir) {
        if let Ok(content) = fs::read_to_string(&desc_path) {
            // Look for <map ... configFilename="...">
            for line in content.lines() {
                if line.contains("<map ") && line.contains("configFilename=") {
                    if let Some(config_path) = extract_attr(line, "configFilename") {
                        let full_path = mod_root.join(&config_path);
                        if full_path.exists() {
                            return Some(full_path);
                        }
                    }
                }
            }
        }
    }

    // For base game maps (no modDesc.xml/dlcDesc.xml), look for mapXX.xml in current directory
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Match mapUS.xml, mapEU.xml, mapAS.xml, etc. but not mapUS_something.xml
            if name_str.starts_with("map") && name_str.ends_with(".xml")
                && !name_str.contains(".i3d")
                && !name_str.contains("_") {
                return Some(entry.path());
            }
        }
    }

    None
}

struct ChannelGroup {
    name: String,
    first_channel: u32,
    num_channels: u32,
    options: Vec<(u32, String)>,
}

fn parse_info_layers(content: &str, config: &MapConfig) -> Vec<LayerSection> {
    let mut sections = Vec::new();

    // Find InfoLayer definitions
    let mut in_info_layer = false;
    let mut current_section: Option<LayerSection> = None;
    let mut current_group: Option<ChannelGroup> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("<InfoLayer ") {
            in_info_layer = true;

            let name = extract_attr(line, "name").unwrap_or_default();
            let file_id = extract_attr(line, "fileId").unwrap_or_default();
            let num_channels: u32 = extract_attr(line, "numChannels")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            // Try to find filename from fileId
            let filename = find_filename_by_id(content, &file_id)
                .unwrap_or_else(|| format!("infoLayer_{}.grle", name));

            current_section = Some(LayerSection {
                name: format!("{} (InfoLayer)", titlecase(&name)),
                filename,
                file_type: "GRLE".to_string(),
                num_channels,
                description: String::new(),
                groups: Vec::new(),
            });

            // Self-closing tag
            if line.ends_with("/>") {
                if let Some(mut section) = current_section.take() {
                    // Add values from config or defaults
                    if section.groups.is_empty() {
                        let group = get_info_layer_values(&name, num_channels, config);
                        if !group.options.is_empty() {
                            section.groups.push(group);
                        }
                    }
                    sections.push(section);
                }
                in_info_layer = false;
            }
        } else if in_info_layer {
            if line.starts_with("<Group ") {
                let name = extract_attr(line, "name").unwrap_or_default();
                let first_channel: u32 = extract_attr(line, "firstChannel")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let num_channels: u32 = extract_attr(line, "numChannels")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);

                current_group = Some(ChannelGroup {
                    name,
                    first_channel,
                    num_channels,
                    options: Vec::new(),
                });
            } else if line.starts_with("<Option ") {
                if let Some(ref mut group) = current_group {
                    let value: u32 = extract_attr(line, "value")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let name = extract_attr(line, "name").unwrap_or_default();
                    group.options.push((value, name));
                }
            } else if line.starts_with("</Group>") {
                if let Some(group) = current_group.take() {
                    if let Some(ref mut section) = current_section {
                        section.groups.push(group);
                    }
                }
            } else if line.starts_with("</InfoLayer>") {
                if let Some(section) = current_section.take() {
                    sections.push(section);
                }
                in_info_layer = false;
            }
        }
    }

    sections
}

fn parse_detail_layers(content: &str, config: &MapConfig) -> Vec<LayerSection> {
    let mut sections = Vec::new();

    let mut in_detail_layer = false;
    let mut current_section: Option<LayerSection> = None;
    let mut current_group: Option<ChannelGroup> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("<DetailLayer ") {
            in_detail_layer = true;

            let name = extract_attr(line, "name").unwrap_or_default();
            let density_map_id = extract_attr(line, "densityMapId").unwrap_or_default();
            let num_channels: u32 = extract_attr(line, "numDensityMapChannels")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            let filename = find_filename_by_id(content, &density_map_id)
                .unwrap_or_else(|| format!("densityMap_{}.gdm", name));

            let display_name = match name.as_str() {
                "terrainDetail" => "Ground (terrainDetail)".to_string(),
                "terrainDetailHeight" => "Height (terrainDetailHeight)".to_string(),
                _ => format!("{} (DetailLayer)", titlecase(&name)),
            };

            current_section = Some(LayerSection {
                name: display_name,
                filename,
                file_type: "GDM".to_string(),
                num_channels,
                description: String::new(),
                groups: Vec::new(),
            });

            if line.ends_with("/>") {
                if let Some(mut section) = current_section.take() {
                    // For height layer without groups, add height-specific info
                    if section.groups.is_empty() && name == "terrainDetailHeight" {
                        // Parse height-specific attributes
                        let height_first: u32 = extract_attr(line, "heightFirstChannel")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(8);
                        let height_num: u32 = extract_attr(line, "heightNumChannels")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(8);
                        let combined: Vec<u32> = extract_attr(line, "combinedValuesChannels")
                            .map(|s| s.split_whitespace().filter_map(|x| x.parse().ok()).collect())
                            .unwrap_or_default();

                        let type_channels = if combined.len() >= 2 { combined[1] } else { height_first };
                        let max_height = extract_attr(line, "maxHeight")
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(4.0);
                        let max_height_val = (1u32 << height_num) - 1;
                        let height_per_unit = max_height / max_height_val as f32;

                        section.description = format!(
                            "Height data for terrain fill (piles).\n\n\
                            - **Fill Type**: Bits 0-{} (R channel, values 0-{})\n\
                            - **Height**: Bits {}-{} (G channel, values 0-{}, representing 0-{:.1}m)\n\n\
                            Paint R with fill type index, G with height value (each unit = {:.3}m).",
                            type_channels - 1,
                            (1u32 << type_channels) - 1,
                            height_first,
                            height_first + height_num - 1,
                            max_height_val,
                            max_height,
                            height_per_unit
                        );

                        // Add fill type group if we have types loaded
                        if config.has_fill_types {
                            section.groups.push(ChannelGroup {
                                name: "Fill Type (R channel)".to_string(),
                                first_channel: 0,
                                num_channels: type_channels,
                                options: config.fill_types.clone(),
                            });
                        } else {
                            // No fill types found - add a note about using --data-dir
                            section.description.push_str("\n\n**Note:** Fill type definitions not found. Use `--data-dir` to specify the base game data folder for fill type names.");
                        }

                        // Add height examples - these will be shown separately since they go in G channel
                        // We use first_channel=0 to show raw values, not shifted
                        let mut height_examples = Vec::new();
                        let example_heights = [0, 1, 10, 25, 50, 63, 100, 127, 200, 255];
                        for &h in &example_heights {
                            if h <= max_height_val {
                                let meters = h as f32 * height_per_unit;
                                let desc = if h == 0 {
                                    "Empty".to_string()
                                } else if h == max_height_val {
                                    format!("{:.1}m (max)", max_height)
                                } else {
                                    format!("{:.2}m", meters)
                                };
                                height_examples.push((h, desc));
                            }
                        }

                        section.groups.push(ChannelGroup {
                            name: "Height (G channel value)".to_string(),
                            first_channel: 0, // Show as raw G value, not shifted
                            num_channels: height_num,
                            options: height_examples,
                        });
                    }
                    sections.push(section);
                }
                in_detail_layer = false;
            }
        } else if in_detail_layer {
            if line.starts_with("<Group ") {
                let name = extract_attr(line, "name").unwrap_or_default();
                let first_channel: u32 = extract_attr(line, "firstChannel")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let num_channels: u32 = extract_attr(line, "numChannels")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);

                current_group = Some(ChannelGroup {
                    name,
                    first_channel,
                    num_channels,
                    options: Vec::new(),
                });
            } else if line.starts_with("<Option ") {
                if let Some(ref mut group) = current_group {
                    let value: u32 = extract_attr(line, "value")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let name = extract_attr(line, "name").unwrap_or_default();
                    group.options.push((value, name));
                }
            } else if line.starts_with("</Group>") {
                if let Some(group) = current_group.take() {
                    if let Some(ref mut section) = current_section {
                        section.groups.push(group);
                    }
                }
            } else if line.starts_with("</DetailLayer>") || line.starts_with("<DistanceTexture") {
                // Save any pending group
                if let Some(group) = current_group.take() {
                    if let Some(ref mut section) = current_section {
                        section.groups.push(group);
                    }
                }

                if line.starts_with("</DetailLayer>") {
                    if let Some(section) = current_section.take() {
                        sections.push(section);
                    }
                    in_detail_layer = false;
                }
            }
        }
    }

    sections
}

fn parse_foliage_layers(content: &str, i3d_path: &str, data_dir: Option<&str>) -> Vec<LayerSection> {
    let mut sections = Vec::new();
    let _i3d_dir = Path::new(i3d_path).parent();

    // Parse FoliageMultiLayer definitions
    let mut in_foliage_multi = false;
    let mut current_density_map_id = String::new();
    let mut current_num_channels: u32 = 0;
    let mut current_type_index_channels: u32 = 0;
    let mut foliage_types: Vec<(String, String)> = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("<FoliageMultiLayer ") {
            in_foliage_multi = true;
            current_density_map_id = extract_attr(line, "densityMapId").unwrap_or_default();
            current_num_channels = extract_attr(line, "numChannels")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            current_type_index_channels = extract_attr(line, "numTypeIndexChannels")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            foliage_types.clear();
        } else if in_foliage_multi {
            if line.starts_with("<FoliageType ") {
                let name = extract_attr(line, "name").unwrap_or_default();
                let xml_id = extract_attr(line, "foliageXmlId").unwrap_or_default();
                foliage_types.push((name, xml_id));
            } else if line.starts_with("</FoliageMultiLayer>") {
                in_foliage_multi = false;

                let filename = find_filename_by_id(content, &current_density_map_id)
                    .unwrap_or_else(|| format!("densityMap_{}.gdm", current_density_map_id));

                // Determine layer name from foliage types
                let layer_name = if foliage_types.len() == 1 {
                    titlecase(&foliage_types[0].0)
                } else if foliage_types.iter().any(|(n, _)| n == "weed") {
                    "Weed".to_string()
                } else if foliage_types.iter().any(|(n, _)| n == "stone") {
                    "Stones".to_string()
                } else if foliage_types.iter().any(|(n, _)| n == "wheat" || n == "grass") {
                    "Fruits/Foliage".to_string()
                } else {
                    format!("Foliage ({})", foliage_types.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", "))
                };

                let mut groups = Vec::new();

                // For type-indexed layers, create type index group
                if current_type_index_channels > 0 {
                    let mut type_options: Vec<(u32, String)> = Vec::new();
                    for (i, (name, _)) in foliage_types.iter().enumerate() {
                        type_options.push((i as u32, titlecase(name)));
                    }

                    groups.push(ChannelGroup {
                        name: "Foliage Type Index".to_string(),
                        first_channel: 0,
                        num_channels: current_type_index_channels,
                        options: type_options,
                    });

                    // State channels
                    let state_channels = current_num_channels - current_type_index_channels;
                    if state_channels > 0 {
                        // Try to load state info from foliage XML files
                        let state_options = load_foliage_states(&foliage_types, content, data_dir);

                        groups.push(ChannelGroup {
                            name: "Growth State".to_string(),
                            first_channel: current_type_index_channels,
                            num_channels: state_channels,
                            options: state_options,
                        });
                    }
                } else {
                    // Single-type layer (weed, stones, etc.)
                    let state_options = load_single_foliage_states(&foliage_types, content, data_dir, &layer_name);

                    if !state_options.is_empty() {
                        groups.push(ChannelGroup {
                            name: "State".to_string(),
                            first_channel: 0,
                            num_channels: current_num_channels,
                            options: state_options,
                        });
                    }
                }

                sections.push(LayerSection {
                    name: format!("{} (FoliageLayer)", layer_name),
                    filename,
                    file_type: "GDM".to_string(),
                    num_channels: current_num_channels,
                    description: if current_type_index_channels > 0 {
                        format!("Contains {} foliage types. Lower {} bits = type index, upper {} bits = growth state.",
                            foliage_types.len(), current_type_index_channels, current_num_channels - current_type_index_channels)
                    } else {
                        String::new()
                    },
                    groups,
                });
            }
        }
    }

    sections
}

fn load_foliage_states(foliage_types: &[(String, String)], i3d_content: &str, data_dir: Option<&str>) -> Vec<(u32, String)> {
    // If data_dir is provided, try to load states from actual foliage XML files
    if let Some(data_path) = data_dir {
        // Try to get states from the first crop type (they all share similar structure)
        for (name, xml_id) in foliage_types {
            // Skip decoration foliage, look for actual crops
            if name.starts_with("deco") || name == "forestPlants" || name == "waterPlants" || name == "meadow" {
                continue;
            }

            if let Some(states) = load_states_from_foliage_xml(i3d_content, xml_id, data_path) {
                if !states.is_empty() {
                    return states;
                }
            }
        }
    }

    // Fall back to common growth states for most crops
    vec![
        (0, "None/Empty".to_string()),
        (1, "Invisible (just planted)".to_string()),
        (2, "Small growth stage 1".to_string()),
        (3, "Small growth stage 2".to_string()),
        (4, "Medium growth stage 1".to_string()),
        (5, "Medium growth stage 2".to_string()),
        (6, "Large growth stage 1".to_string()),
        (7, "Large growth stage 2".to_string()),
        (8, "Harvest ready".to_string()),
        (9, "Withered/Dead".to_string()),
        (10, "Cut/Harvested".to_string()),
    ]
}

/// Load foliage states from the actual XML file
fn load_states_from_foliage_xml(i3d_content: &str, xml_id: &str, data_dir: &str) -> Option<Vec<(u32, String)>> {
    // Find the filename for this foliage XML ID in the i3d
    let search = format!("fileId=\"{}\"", xml_id);
    for line in i3d_content.lines() {
        if line.contains(&search) && line.contains("filename=") {
            if let Some(filename) = extract_attr(line, "filename") {
                // Convert $data path to actual path
                let actual_path = if filename.starts_with("$data/") {
                    format!("{}/{}", data_dir, &filename[6..])
                } else {
                    filename.clone()
                };

                // Try to read and parse the foliage XML
                if let Ok(xml_content) = fs::read_to_string(&actual_path) {
                    return Some(parse_foliage_states(&xml_content));
                }
            }
        }
    }
    None
}

/// Parse foliageState elements from a foliage XML file
fn parse_foliage_states(xml_content: &str) -> Vec<(u32, String)> {
    let mut states = Vec::new();
    let mut state_index = 0u32;

    // First state (index 0) is always "None/Empty" - not in the file
    states.push((0, "None/Empty".to_string()));
    state_index = 1;

    for line in xml_content.lines() {
        let line = line.trim();
        if line.starts_with("<foliageState ") {
            if let Some(name) = extract_attr(line, "name") {
                // Convert camelCase to readable format
                let readable_name = camel_to_readable(&name);

                // Add extra info based on attributes
                let mut description = readable_name;
                if line.contains("isHarvestReady=\"true\"") {
                    description.push_str(" (harvest ready)");
                } else if line.contains("isWithered=\"true\"") {
                    description.push_str(" (withered)");
                } else if line.contains("isCut=\"true\"") {
                    description.push_str(" (cut)");
                } else if line.contains("isGrowing=\"true\"") && !description.to_lowercase().contains("invisible") {
                    description.push_str(" (growing)");
                }

                states.push((state_index, description));
                state_index += 1;
            }
        }
    }

    states
}

/// Convert camelCase to readable format: "greenSmall" -> "Green Small"
fn camel_to_readable(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            result.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            result.push(' ');
            result.push(c);
        } else {
            result.push(c);
        }
    }
    result
}

fn load_single_foliage_states(foliage_types: &[(String, String)], i3d_content: &str, data_dir: Option<&str>, layer_name: &str) -> Vec<(u32, String)> {
    // If data_dir is provided, try to load from actual XML files
    if let Some(data_path) = data_dir {
        if let Some((_, xml_id)) = foliage_types.first() {
            if let Some(states) = load_states_from_foliage_xml(i3d_content, xml_id, data_path) {
                if !states.is_empty() {
                    return states;
                }
            }
        }
    }

    // Fall back to hardcoded defaults
    match layer_name.to_lowercase().as_str() {
        "weed" => vec![
            (0, "None".to_string()),
            (1, "Sparse (invisible)".to_string()),
            (2, "Dense start (invisible)".to_string()),
            (3, "Small weed (visible)".to_string()),
            (4, "Medium weed".to_string()),
            (5, "Large weed".to_string()),
            (6, "Very large weed".to_string()),
            (7, "Herbicided (from small)".to_string()),
            (8, "Herbicided (from medium)".to_string()),
            (9, "Herbicided (from large)".to_string()),
        ],
        "stones" | "stone" => vec![
            (0, "None".to_string()),
            (1, "Mask (picked area marker)".to_string()),
            (2, "Small stones (pickable)".to_string()),
            (3, "Medium stones (pickable)".to_string()),
            (4, "Large stones (pickable)".to_string()),
            (5, "Picked (transitioning)".to_string()),
            (6, "Recently cleared (regenerating)".to_string()),
            (7, "Reserved".to_string()),
        ],
        "deco bush" | "decobush" => vec![
            (0, "None".to_string()),
            (1, "Small bush".to_string()),
            (2, "Medium bush".to_string()),
            (3, "Large bush".to_string()),
        ],
        _ => {
            // Generate generic states based on channel count
            let max_states = 1u32 << 4; // default to 4 bits
            (0..max_states.min(16))
                .map(|i| (i, if i == 0 { "None".to_string() } else { format!("State {}", i) }))
                .collect()
        }
    }
}

/// Get values for info layers - uses config when available, otherwise defaults
fn get_info_layer_values(name: &str, num_channels: u32, config: &MapConfig) -> ChannelGroup {
    let name_lower = name.to_lowercase();

    let options = if name_lower.contains("farmland") {
        // Use farmlands from config if available
        if config.has_farmlands {
            config.farmlands.clone()
        } else {
            // No farmlands config - show generic
            vec![
                (0, "Not owned".to_string()),
                (255, "Special area".to_string()),
            ]
        }
    } else if name_lower.contains("navigationcollision") {
        // Navigation collision layer
        vec![
            (0, "Navigable".to_string()),
            (1, "Blocked".to_string()),
        ]
    } else if name_lower.contains("collision") {
        // Other collision layers: 0 = passable, 1 = blocked
        if name_lower.contains("tipcollisiongenerated") && num_channels >= 2 {
            vec![
                (0, "Default (passable)".to_string()),
                (1, "Blocked".to_string()),
                (2, "Blocked Wall".to_string()),
            ]
        } else {
            vec![
                (0, "Default (passable)".to_string()),
                (1, "Blocked".to_string()),
            ]
        }
    } else if name_lower.contains("indoor") {
        vec![
            (0, "Outdoor".to_string()),
            (1, "Indoor".to_string()),
        ]
    } else if name_lower.contains("spraylevel") {
        // Spray level: 0-3 for fertilizer application count
        vec![
            (0, "Not sprayed".to_string()),
            (1, "Sprayed once".to_string()),
            (2, "Sprayed twice".to_string()),
            (3, "Fully fertilized".to_string()),
        ]
    } else if name_lower.contains("plowlevel") {
        vec![
            (0, "Not plowed".to_string()),
            (1, "Plowed".to_string()),
        ]
    } else if name_lower.contains("rollerlevel") {
        vec![
            (0, "Not rolled".to_string()),
            (1, "Rolled".to_string()),
        ]
    } else if name_lower.contains("limelevel") {
        vec![
            (0, "Needs lime".to_string()),
            (1, "Limed".to_string()),
        ]
    } else if name_lower.contains("stubbleshred") {
        vec![
            (0, "Not shredded".to_string()),
            (1, "Shredded".to_string()),
        ]
    } else if name_lower.contains("weed") && !name_lower.contains("density") {
        // Weed info layer (different from foliage weed)
        vec![
            (0, "No weeds".to_string()),
            (1, "Has weeds".to_string()),
        ]
    } else {
        // Generic binary layer
        let max_val = (1u32 << num_channels) - 1;
        if max_val <= 1 {
            vec![
                (0, "Off".to_string()),
                (1, "On".to_string()),
            ]
        } else {
            // Return empty - we don't know what values mean
            vec![]
        }
    };

    ChannelGroup {
        name: "Values".to_string(),
        first_channel: 0,
        num_channels,
        options,
    }
}

fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    if let Some(start) = line.find(&search) {
        let value_start = start + search.len();
        if let Some(end) = line[value_start..].find('"') {
            return Some(line[value_start..value_start + end].to_string());
        }
    }
    None
}

fn find_filename_by_id(content: &str, file_id: &str) -> Option<String> {
    let search = format!("fileId=\"{}\"", file_id);
    for line in content.lines() {
        if line.contains(&search) && line.contains("filename=") {
            if let Some(filename) = extract_attr(line, "filename") {
                // Extract just the filename part
                let path = Path::new(&filename);
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    // Convert .png reference to actual format
                    if name_str.ends_with(".png") {
                        let base = &name_str[..name_str.len() - 4];
                        if base.contains("infoLayer") {
                            return Some(format!("{}.grle", base));
                        } else if base.contains("densityMap") {
                            return Some(format!("{}.gdm", base));
                        }
                    }
                    return Some(name_str.to_string());
                }
            }
        }
    }
    None
}

fn titlecase(s: &str) -> String {
    let mut result = String::new();
    let mut prev_upper = false;

    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            result.extend(c.to_uppercase());
            prev_upper = c.is_uppercase();
        } else if c.is_uppercase() && !prev_upper {
            result.push(' ');
            result.push(c);
            prev_upper = true;
        } else {
            result.push(c);
            prev_upper = c.is_uppercase();
        }
    }

    result
}
