mod engine;
mod modes;
mod plugins;
mod timeline;
mod world_objects;

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use engine::{GameAppConfig, random_physics_game_app};
use modes::{BenchScene, SimulationMode};
use plugins::run_bench_mode;
use world_objects::{
    ColliderShape, ObjectDef, VehicleDef, VisualDef, preprocess_concave_colliders, spawn_object,
    spawn_staircase, spawn_vehicle,
};

enum DemoCommand {
    Preprocess,
    Sim(SimulationMode),
    Bench { scene: BenchScene, count: u32 },
}

fn parse_demo_command() -> DemoCommand {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--preprocess") {
        return DemoCommand::Preprocess;
    }
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let scene_str = args.get(pos + 1).map(String::as_str).unwrap_or("falling-spheres");
        let count = args.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(100u32);
        let scene = match scene_str {
            "stacked-boxes" => BenchScene::StackedBoxes,
            "chain-grid"    => BenchScene::ChainGrid,
            _               => BenchScene::FallingSpheres,
        };
        return DemoCommand::Bench { scene, count };
    }
    if args.iter().any(|a| a == "--sim-raw") {
        return DemoCommand::Sim(SimulationMode::Raw);
    }
    DemoCommand::Sim(SimulationMode::Precomputed)
}

fn main() {
    match parse_demo_command() {
        DemoCommand::Preprocess         => preprocess_concave_colliders(),
        DemoCommand::Sim(mode)          => run_demo_sim(mode),
        DemoCommand::Bench { scene, count } => run_bench_mode(SimulationMode::Bench { scene, count }),
    }
}

fn run_demo_sim(mode: SimulationMode) {
    random_physics_game_app(mode, GameAppConfig::default())
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
    mode: Res<SimulationMode>,
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
