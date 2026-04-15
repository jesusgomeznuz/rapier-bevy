mod modes;
mod plugins;
mod world_objects;

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_rapier3d::prelude::*;
use modes::{Mode, SimMode, parse_mode};
use plugins::GraphicsPlugin;
use world_objects::{
    ChainDef, ChainPath, ColliderShape, ObjectDef, preprocess_assets, spawn_chain, spawn_object,
};

fn main() {
    match parse_mode() {
        Mode::Preprocess => preprocess_assets(),
        Mode::Sim(sim_mode) => run_simulation(sim_mode),
    }
}

fn run_simulation(mode: SimMode) {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_plugins(GraphicsPlugin)
        .insert_resource(mode)
        .add_systems(Startup, setup_world)
        .add_systems(Update, show_fps_in_title)
        .run();
}

fn show_fps_in_title(
    diagnostics: Res<DiagnosticsStore>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    if let Ok(mut window) = windows.single_mut() {
        window.title = format!("rapier-bevy — {fps:.0} fps");
    }
}

fn setup_world(mut commands: Commands, mode: Res<SimMode>) {
    let start = std::time::Instant::now();

    spawn_object(
        &mut commands,
        ObjectDef {
            shape: ColliderShape::Box {
                hx: 100.0,
                hy: 0.1,
                hz: 100.0,
            },
            position: Vec3::new(0.0, -2.0, 0.0),
            ..Default::default()
        },
        &mode,
    );

    spawn_chain(
        &mut commands,
        ChainDef {
            path: ChainPath::Linear {
                start: Vec3::new(-4.0, 4.5, 0.0),
                direction: Vec3::NEG_Y,
                length: 4.0,
            },
            radius: 0.08,
            anchored: true,
        },
    );

    spawn_chain(
        &mut commands,
        ChainDef {
            path: ChainPath::Linear {
                start: Vec3::new(-1.5, 4.5, 0.0),
                direction: Vec3::X,
                length: 3.0,
            },
            radius: 0.08,
            anchored: true,
        },
    );

    // arco 90° en plano XY, r=3.0 — arco: (π/2)·3 ≈ 4.71m
    spawn_chain(
        &mut commands,
        ChainDef {
            path: ChainPath::Curve {
                sample: Box::new(|t| {
                    let angle = std::f32::consts::FRAC_PI_2 * (1.0 - t);
                    Vec3::new(3.5 + angle.cos() * 3.0, 1.5 + angle.sin() * 3.0, 0.0)
                }),
                length: std::f32::consts::FRAC_PI_2 * 3.0,
            },
            radius: 0.08,
            anchored: true,
        },
    );

    println!("[{}] setup_world: {:.2?}", mode.label(), start.elapsed());
}
