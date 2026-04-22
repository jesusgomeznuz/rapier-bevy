use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use super::{ChainDef, ChainPath, spawn_chain};

pub fn spawn_falling_spheres(commands: &mut Commands, count: u32) {
    let cols = (count as f32).sqrt().ceil() as u32;
    for i in 0..count {
        let x = (i % cols) as f32 * 1.2 - (cols as f32 * 0.6);
        let z = (i / cols) as f32 * 1.2 - (cols as f32 * 0.6);
        let y = 2.0 + (i / cols) as f32 * 0.5;
        commands.spawn((
            Collider::ball(0.5),
            RigidBody::Dynamic,
            Transform::from_xyz(x, y, z),
        ));
    }
}

pub fn spawn_stacked_boxes(commands: &mut Commands, count: u32) {
    let cols = (count as f32).cbrt().ceil() as u32;
    let mut spawned = 0u32;
    'outer: for row in 0..cols {
        for col in 0..cols {
            for stack in 0..cols {
                if spawned >= count {
                    break 'outer;
                }
                commands.spawn((
                    Collider::cuboid(0.5, 0.5, 0.5),
                    RigidBody::Dynamic,
                    Transform::from_xyz(
                        col as f32 * 1.05 - cols as f32 * 0.5,
                        0.5 + stack as f32 * 1.05,
                        row as f32 * 1.05 - cols as f32 * 0.5,
                    ),
                ));
                spawned += 1;
            }
        }
    }
}

pub fn spawn_chain_grid(
    commands: &mut Commands,
    count: u32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let cols = (count as f32).sqrt().ceil() as u32;
    for i in 0..count {
        let x = (i % cols) as f32 * 3.0 - (cols as f32 * 1.5);
        let z = (i / cols) as f32 * 3.0 - (cols as f32 * 1.5);
        spawn_chain(
            commands,
            ChainDef {
                path: ChainPath::Linear {
                    start: Vec3::new(x, 6.0, z),
                    direction: Vec3::NEG_Y,
                    length: 4.0,
                },
                radius: 0.08,
                anchored: true,
                angular_damping: 0.5,
                linear_damping: 0.5,
            },
            meshes,
            materials,
        );
    }
}
