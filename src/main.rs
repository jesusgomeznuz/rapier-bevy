mod plugins;
mod world_objects;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use plugins::GraphicsPlugin;
use world_objects::{preprocess_obj, spawn_object, BodyType, ColliderShape, ObjectDef};

#[derive(Resource)]
enum SimMode { Precomputed, Raw }

enum Mode { Preprocess, Sim(SimMode) }

fn main() {
    match parse_mode() {
        Mode::Preprocess    => preprocess_assets(),
        Mode::Sim(sim_mode) => run_simulation(sim_mode),
    }
}

fn preprocess_assets() {
    let start = std::time::Instant::now();
    preprocess_obj("assets/concave.obj", "assets/concave.compound");
    preprocess_obj("assets/gear.obj",    "assets/gear.compound");
    println!("[preprocess] total: {:.2?}", start.elapsed());
}

fn run_simulation(mode: SimMode) {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_plugins(GraphicsPlugin)
        .insert_resource(mode)
        .add_systems(Startup, setup_world)
        .run();
}

fn setup_world(mut commands: Commands, mode: Res<SimMode>) {
    let start = std::time::Instant::now();
    spawn_object(&mut commands, ObjectDef {
        shape: ColliderShape::Box { hx: 100.0, hy: 0.1, hz: 100.0 },
        position: Vec3::new(0.0, -2.0, 0.0),
        ..Default::default()
    });

    spawn_object(&mut commands, ObjectDef {
        shape: ColliderShape::Sphere { radius: 0.5 },
        position: Vec3::new(0.0, 6.0, 0.0),
        body: BodyType::Dynamic,
        restitution: Some(0.8),
        ..Default::default()
    });

    let concave_shape = match *mode {
        SimMode::Precomputed => ColliderShape::Compound { model_name: "concave" },
        SimMode::Raw         => ColliderShape::RawMesh { obj_path: "assets/concave.obj" },
    };
    spawn_object(&mut commands, ObjectDef {
        shape: concave_shape,
        position: Vec3::new(2.0, 4.0, 0.0),
        body: BodyType::Static,
        ..Default::default()
    });

    let mode_label = match *mode {
        SimMode::Precomputed => "sim-precomputed",
        SimMode::Raw         => "sim-raw",
    };
    println!("[{}] setup_world: {:.2?}", mode_label, start.elapsed());
}

fn parse_mode() -> Mode {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--preprocess".to_string()) {
        Mode::Preprocess
    } else if args.contains(&"--sim-raw".to_string()) {
        Mode::Sim(SimMode::Raw)
    } else {
        Mode::Sim(SimMode::Precomputed)
    }
}
