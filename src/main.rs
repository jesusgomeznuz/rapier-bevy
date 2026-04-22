mod modes;
mod plugins;
mod world_objects;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use modes::{Mode, SimMode, parse_mode};
use plugins::{GraphicsPlugin, PhysicsStatsPlugin, run_bench_mode};
use world_objects::{ColliderShape, ObjectDef, VehicleDef, VisualDef, preprocess_assets,
    spawn_object, spawn_staircase, spawn_vehicle};

fn main() {
    match parse_mode() {
        Mode::Preprocess => preprocess_assets(),
        Mode::Sim(mode @ SimMode::Bench { .. }) => run_bench_mode(mode),
        Mode::Sim(mode) => run_world_mode(mode),
    }
}

fn run_world_mode(mode: SimMode) {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(GraphicsPlugin)
        .add_plugins(PhysicsStatsPlugin)
        .add_systems(Startup, setup_world)
        .insert_resource(mode)
        .run();
}

fn setup_world(
    mut commands: Commands,
    mode: Res<SimMode>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let start = std::time::Instant::now();

    spawn_object(
        &mut commands,
        ObjectDef {
            shape: ColliderShape::Box { hx: 100.0, hy: 0.1, hz: 100.0 },
            visual: Some(VisualDef::grass_green()),
            ..Default::default()
        },
        &mode,
        &asset_server,
        &mut meshes,
        &mut materials,
    );

    spawn_vehicle(
        &mut commands,
        VehicleDef { position: Vec3::new(5.2, 0.65, 0.0) },
        &mode,
        &asset_server,
        &mut meshes,
        &mut materials,
    );

    spawn_staircase(&mut commands, &asset_server, &mode, &mut meshes, &mut materials);

    println!("[{}] setup_world: {:.2?}", mode.label(), start.elapsed());
}
