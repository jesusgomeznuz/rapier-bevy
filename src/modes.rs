use bevy::prelude::*;

#[derive(Clone)]
pub enum BenchScene {
    FallingSpheres,
    StackedBoxes,
    ChainGrid,
}

impl BenchScene {
    pub fn label(&self) -> &'static str {
        match self {
            BenchScene::FallingSpheres => "falling-spheres",
            BenchScene::StackedBoxes => "stacked-boxes",
            BenchScene::ChainGrid => "chain-grid",
        }
    }
}

#[derive(Resource)]
pub enum SimulationMode {
    Precomputed,
    Raw,
    Bench { scene: BenchScene, count: u32 },
}

pub enum EngineMode {
    Preprocess,
    Sim(SimulationMode),
}

impl SimulationMode {
    pub fn label(&self) -> &'static str {
        match self {
            SimulationMode::Precomputed => "sim-precomputed",
            SimulationMode::Raw => "sim-raw",
            SimulationMode::Bench { scene, .. } => scene.label(),
        }
    }
}

pub fn debug_enabled() -> bool {
    std::env::args().any(|a| a == "--debug")
}

pub fn record_duration() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--record")?;
    let secs = args
        .get(pos + 1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u32);
    Some(secs)
}

pub fn write_timeline_duration() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    reject_renamed_flag(&args, "--bake", "--write-timeline");
    reject_renamed_flag(&args, "--simulate", "--write-timeline");
    let pos = args.iter().position(|a| a == "--write-timeline")?;
    let secs = args
        .get(pos + 1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u32);
    Some(secs)
}

pub fn play_path() -> Option<std::path::PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    reject_renamed_flag(&args, "--replay", "--play");
    let pos = args.iter().position(|a| a == "--play")?;
    let path = args
        .get(pos + 1)
        .expect("--play requiere la ruta de la timeline (ej. outputs/simulation_60s.timeline)");
    Some(std::path::PathBuf::from(path))
}

/// La misma regla con la que el engine arma el mundo: hay física salvo que se
/// esté reproduciendo una timeline. El juego la consulta para decidir si
/// escucha colisiones reales.
pub fn no_timeline_is_playing() -> bool {
    play_path().is_none()
}

pub fn parse_engine_mode(args: &[String]) -> EngineMode {
    if args.iter().any(|a| a == "--preprocess") {
        return EngineMode::Preprocess;
    }
    if args.iter().any(|a| a == "--sim-raw") {
        return EngineMode::Sim(SimulationMode::Raw);
    }
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let scene_str = args
            .get(pos + 1)
            .map(String::as_str)
            .unwrap_or("falling-spheres");
        let count = args
            .get(pos + 2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(100u32);
        let scene = match scene_str {
            "stacked-boxes" => BenchScene::StackedBoxes,
            "chain-grid" => BenchScene::ChainGrid,
            _ => BenchScene::FallingSpheres,
        };
        return EngineMode::Sim(SimulationMode::Bench { scene, count });
    }
    EngineMode::Sim(SimulationMode::Precomputed)
}

fn reject_renamed_flag(args: &[String], old: &str, new: &str) {
    if args.iter().any(|a| a == old) {
        eprintln!("{old} fue renombrado: usa {new}");
        std::process::exit(1);
    }
}
