mod modes;
mod plugins;
mod world_objects;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use modes::{parse_mode, Mode, SimMode};
use plugins::GraphicsPlugin;
use world_objects::{preprocess_assets, spawn_object, BodyType, ColliderShape, JointDef, ObjectDef};

fn main() {
    match parse_mode() {
        Mode::Preprocess    => preprocess_assets(),
        Mode::Sim(sim_mode) => run_simulation(sim_mode),
    }
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
    }, &mode);

    spawn_object(&mut commands, ObjectDef {
        shape: ColliderShape::Box { hx: 1.5, hy: 0.05, hz: 0.1 },
        position: Vec3::new(0.0, 2.0, 0.0),
        body: BodyType::Dynamic,
        joint: Some(JointDef::Revolute {
            axis: Vec3::Z,
            local_anchor: Vec3::new(-1.5, 0.0, 0.0),
        }),
        ..Default::default()
    }, &mode);

    println!("[{}] setup_world: {:.2?}", mode.label(), start.elapsed());
}
