mod modes;
mod plugins;
mod world_objects;

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use modes::{Mode, SimMode, parse_mode};
use plugins::GraphicsPlugin;
use modes::BenchScene;
use world_objects::{
    ChainDef, ChainPath, ColliderShape, ObjectDef, preprocess_assets,
    spawn_chain, spawn_object, spawn_falling_spheres, spawn_stacked_boxes, spawn_chain_grid,
};

fn main() {
    match parse_mode() {
        Mode::Preprocess => preprocess_assets(),
        Mode::Sim(sim_mode) => run_simulation(sim_mode),
    }
}

#[derive(Component)]
struct PerfOverlay;

const BENCH_WARMUP_FRAMES: u32  = 120;
const BENCH_MEASURE_FRAMES: u32 = 600;

#[derive(Resource, Default)]
struct BenchState {
    frame:       u32,
    fps_samples: Vec<f64>,
}

fn setup_bench(mut commands: Commands, mode: Res<SimMode>) {
    let SimMode::Bench { scene, count } = &*mode else { return };

    // suelo compartido en todas las escenas
    commands.spawn((
        Collider::cuboid(200.0, 0.1, 200.0),
        Transform::from_xyz(0.0, -1.0, 0.0),
    ));

    match scene {
        BenchScene::FallingSpheres => spawn_falling_spheres(&mut commands, *count),
        BenchScene::StackedBoxes   => spawn_stacked_boxes(&mut commands, *count),
        BenchScene::ChainGrid      => spawn_chain_grid(&mut commands, *count),
    }

    println!("bench,scene,count,fps_avg,fps_p01");
}

fn run_bench(
    diagnostics: Res<DiagnosticsStore>,
    mode:        Res<SimMode>,
    mut state:   ResMut<BenchState>,
    mut exit:    EventWriter<AppExit>,
) {
    state.frame += 1;

    if state.frame <= BENCH_WARMUP_FRAMES {
        return;
    }

    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.value())
    {
        state.fps_samples.push(fps);
    }

    if state.frame >= BENCH_WARMUP_FRAMES + BENCH_MEASURE_FRAMES {
        let SimMode::Bench { scene, count } = &*mode else { return };
        let samples = &state.fps_samples;

        let fps_avg = samples.iter().sum::<f64>() / samples.len() as f64;

        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p01_idx = ((sorted.len() as f32 * 0.01) as usize).max(1) - 1;
        let fps_p01 = sorted[p01_idx];

        println!("bench,{},{},{:.1},{:.1}", scene.label(), count, fps_avg, fps_p01);
        exit.write(AppExit::Success);
    }
}

fn run_simulation(mode: SimMode) {
    let is_bench = matches!(mode, SimMode::Bench { .. });
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(GraphicsPlugin)
        .insert_resource(mode);

    if !is_bench {
        app.add_plugins(RapierDebugRenderPlugin::default());
    }

    if is_bench {
        app.insert_resource(BenchState::default())
            .add_systems(Startup, (setup_bench, setup_perf_overlay))
            .add_systems(Update, (update_perf_overlay, run_bench));
    } else {
        app.add_systems(Startup, (setup_world, setup_perf_overlay))
            .add_systems(Update, update_perf_overlay);
    }

    app.run();
}

fn setup_perf_overlay(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top:  Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        PerfOverlay,
    ));
}

fn update_perf_overlay(
    diagnostics: Res<DiagnosticsStore>,
    bodies:      Query<&RigidBody>,
    joints:      Query<&ImpulseJoint>,
    mut overlay: Query<&mut Text, With<PerfOverlay>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let n_dynamic = bodies.iter().filter(|rb| matches!(rb, RigidBody::Dynamic)).count();
    let n_joints  = joints.iter().count();

    if let Ok(mut text) = overlay.single_mut() {
        **text = format!("{fps:.0} fps\nbodies  {n_dynamic}\njoints  {n_joints}");
    }
}

fn setup_world(mut commands: Commands, mode: Res<SimMode>) {
    let start = std::time::Instant::now();

    let angular_damping = 0.6_f32;
    let linear_damping = 0.6_f32;

    spawn_object(
        &mut commands,
        ObjectDef {
            shape: ColliderShape::Box {
                hx: 100.0,
                hy: 0.1,
                hz: 100.0,
            },
            position: Vec3::new(0.0, -3.0, 0.0),
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
            angular_damping,
            linear_damping,
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
            angular_damping,
            linear_damping,
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
            angular_damping,
            linear_damping,
        },
    );

    println!("[{}] setup_world: {:.2?}", mode.label(), start.elapsed());
}
