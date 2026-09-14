use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

use city_game_core::{CityScenario, build_render_frame, import_osm_pbf_bytes};

const DEFAULT_FRAME_ASPECT: f32 = 16.0 / 9.0;

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, command, input, output] if command == "import" => import(input, output),
        [_, command, input, output] if command == "frame" => {
            frame(input, output, DEFAULT_FRAME_ASPECT)
        }
        [_, command, input, output, aspect] if command == "frame" => {
            frame(input, output, aspect.parse::<f32>()?)
        }
        _ => {
            eprintln!("usage:");
            eprintln!("  city-game-cli import <input.osm.pbf> <scenario.json>");
            eprintln!("  city-game-cli frame <scenario.json> <frame.json> [aspect]");
            std::process::exit(2);
        }
    }
}

fn import(input: &str, output: &str) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(input)?;
    let source_name = Path::new(input)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(input);
    let scenario = import_osm_pbf_bytes(source_name, &bytes)?;
    write_json(output, &scenario)
}

fn frame(input: &str, output: &str, aspect: f32) -> Result<(), Box<dyn Error>> {
    let scenario: CityScenario = serde_json::from_slice(&fs::read(input)?)?;
    let frame = build_render_frame(&scenario, aspect)?;
    write_json(output, &frame)
}

fn write_json(path: &str, value: &impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
