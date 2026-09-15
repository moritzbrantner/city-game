use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

use city_game_core::{
    CityCommand, CityQuery, CitySave, CityScenario, CityTimeConfig, OsmScenarioTransformConfig,
    build_render_frame, import_osm_pbf_bytes,
};

const DEFAULT_FRAME_ASPECT: f32 = 16.0 / 9.0;

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, command, input, output] if command == "import" => import(input, output, None),
        [_, command, input, output, transform_config] if command == "import" => {
            import(input, output, Some(transform_config))
        }
        [_, command, input, output] if command == "new-save" => {
            new_save(input, output, CityTimeConfig::default())
        }
        [_, command, input, output, minutes_per_tick] if command == "new-save" => new_save(
            input,
            output,
            CityTimeConfig::new(minutes_per_tick.parse::<u16>()?)?,
        ),
        [_, command, input, output, steps] if command == "step" => {
            step_save(input, output, steps.parse::<u64>()?)
        }
        [_, command, save, request, output_save, outcome] if command == "command" => {
            execute_command(save, request, output_save, outcome)
        }
        [_, command, save, request, result] if command == "query" => query(save, request, result),
        [_, command, input, output] if command == "frame" => {
            frame(input, output, DEFAULT_FRAME_ASPECT)
        }
        [_, command, input, output, aspect] if command == "frame" => {
            frame(input, output, aspect.parse::<f32>()?)
        }
        _ => {
            eprintln!("usage:");
            eprintln!(
                "  city-game-cli import <input.osm.pbf> <scenario.json> [transform-config.json]"
            );
            eprintln!("  city-game-cli new-save <scenario.json> <save.json> [minutes-per-tick]");
            eprintln!("  city-game-cli step <save.json> <output-save.json> <steps>");
            eprintln!(
                "  city-game-cli command <save.json> <command.json> <output-save.json> <outcome.json>"
            );
            eprintln!("  city-game-cli query <save.json> <query.json> <result.json>");
            eprintln!("  city-game-cli frame <scenario.json> <frame.json> [aspect]");
            std::process::exit(2);
        }
    }
}

fn import(input: &str, output: &str, transform_config: Option<&str>) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(input)?;
    let source_name = Path::new(input)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(input);
    let scenario = import_osm_pbf_bytes(source_name, &bytes)?;
    let scenario = match transform_config {
        Some(path) => {
            let config: OsmScenarioTransformConfig = read_json(path)?;
            config.transform(scenario)?
        }
        None => scenario,
    };
    write_json(output, &scenario)
}

fn new_save(input: &str, output: &str, time: CityTimeConfig) -> Result<(), Box<dyn Error>> {
    let scenario: CityScenario = read_json(input)?;
    scenario.validate_schema()?;
    let save = CitySave::new_with_time_config(scenario, time)?;
    write_json(output, &save)
}

fn step_save(input: &str, output: &str, steps: u64) -> Result<(), Box<dyn Error>> {
    let mut save: CitySave = read_json(input)?;
    save.advance_fixed_steps(steps)?;
    write_json(output, &save)
}

fn execute_command(
    save_path: &str,
    command_path: &str,
    output_save: &str,
    outcome_path: &str,
) -> Result<(), Box<dyn Error>> {
    let mut save: CitySave = read_json(save_path)?;
    let command: CityCommand = read_json(command_path)?;
    let outcome = save.execute(command)?;

    // Do not replace the authoritative save unless the separate outcome can be persisted first.
    write_json(outcome_path, &outcome)?;
    write_json(output_save, &save)
}

fn query(save_path: &str, query_path: &str, result_path: &str) -> Result<(), Box<dyn Error>> {
    let save: CitySave = read_json(save_path)?;
    let query: CityQuery = read_json(query_path)?;
    let result = save.query(query)?;
    write_json(result_path, &result)
}

fn frame(input: &str, output: &str, aspect: f32) -> Result<(), Box<dyn Error>> {
    let scenario: CityScenario = read_json(input)?;
    scenario.validate_schema()?;
    let frame = build_render_frame(&scenario, aspect)?;
    write_json(output, &frame)
}

fn read_json<T>(path: &str) -> Result<T, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json(path: &str, value: &impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
