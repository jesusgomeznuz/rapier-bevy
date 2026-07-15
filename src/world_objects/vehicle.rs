use super::{BodyType, ColliderShape, ObjectDef, VisualDef, spawn_object};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy_rapier3d::geometry::Group;
use bevy_rapier3d::prelude::*;
use std::f32::consts::FRAC_PI_2;

pub struct VehicleDef {
    pub position: Vec3,
}

pub fn spawn_vehicle(
    commands: &mut Commands,
    def: VehicleDef,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let rotation = Quat::from_rotation_y(-FRAC_PI_2);
    let collision = CollisionGroups::new(Group::GROUP_2, Group::ALL ^ Group::GROUP_2);

    let chassis = spawn_chassis(
        commands,
        &def,
        asset_server,
        meshes,
        materials,
        rotation,
        collision,
    );
    let wheels = spawn_wheels(
        commands,
        &def,
        asset_server,
        meshes,
        materials,
        rotation,
        collision,
    );
    attach_wheels_to_chassis(commands, chassis, wheels);
}

fn spawn_chassis(
    commands: &mut Commands,
    def: &VehicleDef,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    rotation: Quat,
    collision: CollisionGroups,
) -> Entity {
    spawn_object(
        commands,
        ObjectDef {
            shape: ColliderShape::MeshObject {
                model_name: "vehicle-racer-chassis".into(),
            },
            position: def.position,
            rotation,
            body: BodyType::Dynamic,
            friction: Some(0.3),
            restitution: Some(0.0),
            angular_damping: Some(2.0),
            collision_groups: Some(collision),
            visual: Some(VisualDef::white_matte()),
            ..Default::default()
        },
        asset_server,
        meshes,
        materials,
    )
}

fn spawn_wheels(
    commands: &mut Commands,
    def: &VehicleDef,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    rotation: Quat,
    collision: CollisionGroups,
) -> [(Entity, Vec3, bool); 4] {
    let wheel_offsets = [
        (Vec3::new(-0.193, 0.122, 0.250), true),   // trasera izq
        (Vec3::new(0.193, 0.122, 0.250), true),    // trasera der
        (Vec3::new(-0.193, 0.122, -0.250), false), // delantera izq
        (Vec3::new(0.193, 0.122, -0.250), false),  // delantera der
    ];

    wheel_offsets.map(|(offset, powered)| {
        let entity = spawn_object(
            commands,
            ObjectDef {
                shape: ColliderShape::Cylinder {
                    half_height: 0.078,
                    radius: 0.156,
                    axis: Vec3::X,
                },
                position: def.position + rotation * offset,
                rotation,
                body: BodyType::Dynamic,
                friction: Some(0.8),
                restitution: Some(0.0),
                angular_damping: Some(30.0),
                collision_groups: Some(collision),
                visual: Some(VisualDef::from_scene("wheel-medium.glb#Scene0")),
                ..Default::default()
            },
            asset_server,
            meshes,
            materials,
        );
        (entity, offset, powered)
    })
}

fn attach_wheels_to_chassis(
    commands: &mut Commands,
    chassis: Entity,
    wheels: [(Entity, Vec3, bool); 4],
) {
    for (wheel, offset, powered) in wheels {
        let mut builder = RevoluteJointBuilder::new(Vec3::X)
            .local_anchor1(offset)
            .local_anchor2(Vec3::ZERO);
        if powered {
            builder = builder.motor_velocity(0.0, 50.0);
        }
        commands
            .entity(wheel)
            .insert(ImpulseJoint::new(chassis, builder.build()));
    }
}
