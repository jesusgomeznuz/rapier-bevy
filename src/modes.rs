use bevy::prelude::*;

#[derive(Resource)]
pub enum SimMode {
    Precomputed,
    Raw,
}

pub enum Mode {
    Preprocess,
    Sim(SimMode),
}

impl SimMode {
    pub fn label(&self) -> &'static str {
        match self {
            SimMode::Precomputed => "sim-precomputed",
            SimMode::Raw         => "sim-raw",
        }
    }
}

pub fn parse_mode() -> Mode {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--preprocess".to_string()) {
        Mode::Preprocess
    } else if args.contains(&"--sim-raw".to_string()) {
        Mode::Sim(SimMode::Raw)
    } else {
        Mode::Sim(SimMode::Precomputed)
    }
}
