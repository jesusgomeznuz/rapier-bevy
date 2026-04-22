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

pub enum Mode {
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

pub fn parse_mode() -> Mode {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--preprocess".to_string()) {
        Mode::Preprocess
    } else if args.contains(&"--sim-raw".to_string()) {
        Mode::Sim(SimMode::Raw)
    } else if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let scene_str = args.get(pos + 1).map(String::as_str).unwrap_or("falling-spheres");
        let count     = args.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(100u32);
        let scene = match scene_str {
            "stacked-boxes" => BenchScene::StackedBoxes,
            "chain-grid"    => BenchScene::ChainGrid,
            _               => BenchScene::FallingSpheres,
        };
        Mode::Sim(SimMode::Bench { scene, count })
    } else {
        Mode::Sim(SimMode::Precomputed)
    }
}
