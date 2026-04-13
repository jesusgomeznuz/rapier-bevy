mod colliders;

use crate::modes::SimMode;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

#[allow(dead_code)]
pub enum ColliderShape {
    Box { hx: f32, hy: f32, hz: f32 },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
    MeshObject { model_name: &'static str },
}

#[allow(dead_code)]
pub enum BodyType {
    Static,
    Dynamic,
}

pub enum JointDef {
    Revolute { axis: Vec3, local_anchor: Vec3 },
}

pub struct ObjectDef {
    pub shape: ColliderShape,
    pub position: Vec3,
    pub body: BodyType,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    pub joint: Option<JointDef>,
}

impl Default for ObjectDef {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Box { hx: 1.0, hy: 1.0, hz: 1.0 },
            position: Vec3::ZERO,
            body: BodyType::Static,
            friction: None,
            restitution: None,
            joint: None,
        }
    }
}

pub fn preprocess_assets() {
    let start = std::time::Instant::now();
    colliders::preprocess_obj("assets/half_ring.obj", "assets/half_ring.compound");
    colliders::preprocess_obj("assets/gear.obj",      "assets/gear.compound");
    println!("[preprocess] total: {:.2?}", start.elapsed());
}

pub fn spawn_object(commands: &mut Commands, def: ObjectDef, mode: &SimMode) {
    let anchor = def.joint.as_ref().map(|joint| match joint {
        JointDef::Revolute { local_anchor, .. } => commands
            .spawn((RigidBody::Fixed, Transform::from_translation(def.position + *local_anchor)))
            .id(),
    });

    let mut entity = commands.spawn((
        colliders::build_collider(def.shape, mode),
        Transform::from_translation(def.position),
    ));

    if let BodyType::Dynamic = def.body {
        entity.insert(RigidBody::Dynamic);
    }
    if let Some(f) = def.friction {
        entity.insert(Friction::coefficient(f));
    }
    if let Some(r) = def.restitution {
        entity.insert(Restitution::coefficient(r));
    }
    if let (Some(joint_def), Some(anchor_entity)) = (def.joint, anchor) {
        match joint_def {
            JointDef::Revolute { axis, local_anchor } => {
                entity.insert(ImpulseJoint::new(anchor_entity, RevoluteJointBuilder::new(axis)
                    .local_anchor1(Vec3::ZERO)
                    .local_anchor2(local_anchor)
                    .build()));
            }
        }
    }
}
