use crate::modes::{BenchScene, SimulationMode};
use crate::plugins::PhysicsStatsPlugin;
use crate::world_objects::{spawn_chain_grid, spawn_falling_spheres, spawn_stacked_boxes};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

pub fn run_bench_mode(mode: SimulationMode) {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(PhysicsStatsPlugin)
        .add_plugins(BenchmarkPlugin)
        .insert_resource(mode)
        .run();
}

pub struct BenchmarkPlugin;

impl Plugin for BenchmarkPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BenchState::default())
            .add_systems(Startup, setup_bench)
            .add_systems(Update, run_bench);
    }
}

#[derive(Resource, Default)]
struct BenchState {
    frame: u32,
    fps_samples: Vec<f64>,
}

fn setup_bench(
    mut commands: Commands,
    mode: Res<SimulationMode>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let SimulationMode::Bench { scene, count } = &*mode else {
        return;
    };

    commands.spawn((
        Collider::cuboid(200.0, 0.1, 200.0),
        Transform::from_xyz(0.0, -1.0, 0.0),
    ));

    match scene {
        BenchScene::FallingSpheres => spawn_falling_spheres(&mut commands, *count),
        BenchScene::StackedBoxes => spawn_stacked_boxes(&mut commands, *count),
        BenchScene::ChainGrid => {
            spawn_chain_grid(&mut commands, *count, &mut meshes, &mut materials)
        }
    }

    println!("bench,scene,count,fps_avg,fps_p01");
}

fn run_bench(
    diagnostics: Res<DiagnosticsStore>,
    mode: Res<SimulationMode>,
    mut state: ResMut<BenchState>,
    mut exit: EventWriter<AppExit>,
) {
    let warmup_frames = 120;
    let measure_frames = 600;

    state.frame += 1;

    if state.frame <= warmup_frames {
        return;
    }

    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.value())
    {
        state.fps_samples.push(fps);
    }

    if state.frame >= warmup_frames + measure_frames {
        let SimulationMode::Bench { scene, count } = &*mode else {
            return;
        };
        let samples = &state.fps_samples;
        let fps_avg = samples.iter().sum::<f64>() / samples.len() as f64;

        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p01_idx = ((sorted.len() as f32 * 0.01) as usize).max(1) - 1;
        let fps_p01 = sorted[p01_idx];

        println!(
            "bench,{},{},{:.1},{:.1}",
            scene.label(),
            count,
            fps_avg,
            fps_p01
        );
        exit.write(AppExit::Success);
    }
}
