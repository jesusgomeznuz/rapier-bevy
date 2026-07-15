mod engine;
mod modes;
mod plugins;
mod timeline;
mod world_objects;

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use engine::{GameAppConfig, random_physics_game_app};
use world_objects::{
    ColliderShape, ObjectDef, VehicleDef, VisualDef, preprocess_concave_colliders, spawn_object,
    spawn_staircase, spawn_vehicle,
};

enum DemoCommand {
    Preprocess,
    Sim,
}

fn parse_demo_command() -> DemoCommand {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--preprocess") {
        return DemoCommand::Preprocess;
    }
    DemoCommand::Sim
}

fn main() {
    match parse_demo_command() {
        DemoCommand::Preprocess => preprocess_concave_colliders(),
        DemoCommand::Sim        => run_demo_sim(),
    }
}

fn run_demo_sim() {
    random_physics_game_app(GameAppConfig::default())
        .add_systems(Startup, (spawn_demo_camera, setup_world))
        .run();
}

fn spawn_demo_camera(mut commands: Commands, offscreen: Option<Res<plugins::record::OffscreenTarget>>) {
    // En modo --record la cámara debe renderizar al OffscreenTarget (no hay ventana).
    let mut camera = Camera::default();
    if let Some(off) = &offscreen {
        camera.target = bevy::render::camera::RenderTarget::Image(off.image.clone().into());
    }

    commands.spawn((
        Camera3d::default(),
        camera,
        Transform::from_xyz(0.0, 13.0, 22.0).looking_at(Vec3::new(0.0, 12.0, 0.0), Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.5, -0.3, 0.0)),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 120.0,
        ..default()
    });
}

fn setup_world(
    mut commands: Commands,
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
        &asset_server,
        &mut meshes,
        &mut materials,
    );

    spawn_vehicle(
        &mut commands,
        VehicleDef { position: Vec3::new(5.2, 0.65, 0.0) },
        &asset_server,
        &mut meshes,
        &mut materials,
    );

    spawn_staircase(&mut commands, &asset_server, &mut meshes, &mut materials);

    println!("[demo] setup_world: {:.2?}", start.elapsed());
}
