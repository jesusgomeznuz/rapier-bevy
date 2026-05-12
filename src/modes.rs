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
            BenchScene::StackedBoxes   => "stacked-boxes",
            BenchScene::ChainGrid      => "chain-grid",
        }
    }
}

#[derive(Resource)]
pub enum SimMode {
    Precomputed,
    Raw,
    Bench { scene: BenchScene, count: u32 },
}

pub enum EngineMode {
    Preprocess,
    Sim(SimMode),
}

impl SimMode {
    pub fn label(&self) -> &'static str {
        match self {
            SimMode::Precomputed          => "sim-precomputed",
            SimMode::Raw                  => "sim-raw",
            SimMode::Bench { scene, .. }  => scene.label(),
        }
    }
}

pub fn debug_enabled() -> bool {
    std::env::args().any(|a| a == "--debug")
}

pub fn record_duration() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    let pos  = args.iter().position(|a| a == "--record")?;
    let secs = args.get(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(60u32);
    Some(secs)
}

pub fn parse_engine_mode(args: &[String]) -> EngineMode {
    if args.iter().any(|a| a == "--preprocess") {
        return EngineMode::Preprocess;
    }
    if args.iter().any(|a| a == "--sim-raw") {
        return EngineMode::Sim(SimMode::Raw);
    }
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let scene_str = args.get(pos + 1).map(String::as_str).unwrap_or("falling-spheres");
        let count     = args.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(100u32);
        let scene = match scene_str {
            "stacked-boxes" => BenchScene::StackedBoxes,
            "chain-grid"    => BenchScene::ChainGrid,
            _               => BenchScene::FallingSpheres,
        };
        return EngineMode::Sim(SimMode::Bench { scene, count });
    }
    EngineMode::Sim(SimMode::Precomputed)
}
