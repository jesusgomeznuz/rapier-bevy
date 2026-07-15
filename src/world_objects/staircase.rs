use super::{BodyType, ColliderShape, ObjectDef, VisualDef, spawn_object};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

pub fn spawn_staircase(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let steps           = 15;
    let domino_steps    = 12;
    let step_width      = 2.0;
    let step_depth      = 0.40;
    let step_height     = 0.15;
    let domino_hx       = 0.025;
    let domino_hy       = 0.22;
    let domino_hz       = 0.65;
    let ball_radius     = 0.10;
    let platform_length = 1.5;
    let floor_y         = 0.1;
    let ramp_horizontal = 3.2;
    let ramp_hy         = 0.05;

    spawn_launch_platform(commands, asset_server, meshes, materials, steps, step_height, step_width, platform_length);
    spawn_steps(commands, asset_server, meshes, materials, steps, step_depth, step_height, step_width);
    spawn_dominoes(commands, asset_server, meshes, materials, steps, domino_steps, step_depth, step_height, domino_hx, domino_hy, domino_hz);
    spawn_exit_ramp(commands, asset_server, meshes, materials, steps, domino_steps, step_depth, step_height, floor_y, ramp_horizontal, ramp_hy, step_width);
    spawn_trigger_ball(commands, asset_server, meshes, materials, steps, step_height, platform_length, ball_radius);
}

fn spawn_launch_platform(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    steps: usize, step_height: f32, step_width: f32, platform_length: f32,
) {
    let top_surface = steps as f32 * step_height;
    let half_height = top_surface / 2.0;
    spawn_object(
        commands,
        ObjectDef {
            shape: ColliderShape::Box {
                hx: platform_length / 2.0,
                hy: half_height,
                hz: step_width / 2.0,
            },
            position: Vec3::new(-platform_length / 2.0, half_height, 0.0),
            friction: Some(0.6),
            visual: Some(VisualDef::white_matte()),
            ..Default::default()
        },
        asset_server, meshes, materials,
    );
}

fn spawn_steps(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    steps: usize, step_depth: f32, step_height: f32, step_width: f32,
) {
    for i in 0..steps {
        let surface_y       = (steps - i) as f32 * step_height;
        let step_half_height = surface_y / 2.0;
        let step_center_x   = i as f32 * step_depth + step_depth / 2.0;
        spawn_object(
            commands,
            ObjectDef {
                shape: ColliderShape::Box {
                    hx: step_depth / 2.0,
                    hy: step_half_height,
                    hz: step_width / 2.0,
                },
                position: Vec3::new(step_center_x, step_half_height, 0.0),
                visual: Some(VisualDef::white_matte()),
                ..Default::default()
            },
            asset_server, meshes, materials,
        );
    }
}

fn spawn_dominoes(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    steps: usize, domino_steps: usize,
    step_depth: f32, step_height: f32,
    domino_hx: f32, domino_hy: f32, domino_hz: f32,
) {
    for i in 0..domino_steps {
        let surface_y      = (steps - i) as f32 * step_height;
        let domino_center_x = i as f32 * step_depth + step_depth * 0.70;
        spawn_object(
            commands,
            ObjectDef {
                shape: ColliderShape::Box { hx: domino_hx, hy: domino_hy, hz: domino_hz },
                position: Vec3::new(domino_center_x, surface_y + domino_hy, 0.0),
                body: BodyType::Dynamic,
                friction: Some(0.5),
                restitution: Some(0.05),
                visual: Some(VisualDef::white_matte()),
                ..Default::default()
            },
            asset_server, meshes, materials,
        );
    }
}

fn spawn_exit_ramp(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    steps: usize, domino_steps: usize,
    step_depth: f32, step_height: f32, floor_y: f32,
    ramp_horizontal: f32, ramp_hy: f32, step_width: f32,
) {
    let ramp_start_x  = domino_steps as f32 * step_depth;
    let ramp_start_y  = (steps - domino_steps) as f32 * step_height;
    let height_diff   = ramp_start_y - floor_y - 2.0 * ramp_hy;
    let ramp_angle    = (height_diff / ramp_horizontal).atan();
    let ramp_center_x = ramp_start_x + ramp_horizontal / 2.0;
    let ramp_center_y = (ramp_start_y + floor_y) / 2.0;
    spawn_object(
        commands,
        ObjectDef {
            shape: ColliderShape::Box {
                hx: ramp_horizontal / 2.0,
                hy: ramp_hy,
                hz: step_width / 2.0,
            },
            position: Vec3::new(ramp_center_x, ramp_center_y, 0.0),
            rotation: Quat::from_rotation_z(-ramp_angle),
            friction: Some(0.4),
            visual: Some(VisualDef::white_matte()),
            ..Default::default()
        },
        asset_server, meshes, materials,
    );
}

fn spawn_trigger_ball(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    steps: usize, step_height: f32, platform_length: f32, ball_radius: f32,
) {
    let top_surface = steps as f32 * step_height;
    spawn_object(
        commands,
        ObjectDef {
            shape: ColliderShape::Sphere { radius: ball_radius },
            position: Vec3::new(-platform_length * 0.6, top_surface + ball_radius + 0.01, 0.0),
            body: BodyType::Dynamic,
            friction: Some(0.8),
            restitution: Some(0.3),
            velocity: Some(Vec3::new(5.0, 0.0, 0.0)),
            visual: Some(VisualDef::gold()),
            ..Default::default()
        },
        asset_server, meshes, materials,
    );
}
