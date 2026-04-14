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

pub enum ChainPath {
    Linear { start: Vec3, direction: Vec3, length: f32 },
    Curve  { sample: Box<dyn Fn(f32) -> Vec3>, length: f32 },
}

pub struct ChainDef {
    pub path:     ChainPath,
    pub segments: u32,
    pub radius:   f32,
    pub anchored: bool,
}

pub struct ObjectDef {
    pub shape: ColliderShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub body: BodyType,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    pub angular_damping: Option<f32>,
    pub joint: Option<JointDef>,
}

impl Default for ObjectDef {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Box { hx: 1.0, hy: 1.0, hz: 1.0 },
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            body: BodyType::Static,
            friction: None,
            restitution: None,
            angular_damping: None,
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
        Transform::from_translation(def.position).with_rotation(def.rotation),
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
    if let Some(d) = def.angular_damping {
        entity.insert(Damping { angular_damping: d, linear_damping: 0.0 });
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

pub fn spawn_chain(commands: &mut Commands, def: ChainDef) {
    let (total_length, anchor_point) = match &def.path {
        ChainPath::Linear { length, start, .. } => (*length, *start),
        ChainPath::Curve  { length, sample }    => (*length, sample(0.0)),
    };

    let link_length = total_length / def.segments as f32;
    let link_half   = link_length / 2.0;
    let half_height = (link_half - def.radius).max(0.001);
    let collider    = Collider::capsule_y(half_height, def.radius);

    let hinge_axis = match &def.path {
        ChainPath::Linear { direction, .. } => {
            let dir = direction.normalize();
            let perpendicular = dir.cross(Vec3::Y);
            if perpendicular.length_squared() > 0.001 { perpendicular.normalize() } else { Vec3::Z }
        }
        ChainPath::Curve { .. } => Vec3::Z,
    };

    // prev: (Entity, center en mundo, rotación en mundo)
    let mut prev: Option<(Entity, Vec3, Quat)> = if def.anchored {
        let e = commands.spawn((RigidBody::Fixed, Transform::from_translation(anchor_point))).id();
        Some((e, anchor_point, Quat::IDENTITY))
    } else {
        None
    };

    for i in 0..def.segments {
        let t   = (i as f32 + 0.5) / def.segments as f32;
        let eps = 0.5 / def.segments as f32;

        let (center, tangent) = match &def.path {
            ChainPath::Linear { start, direction, .. } => {
                let dir = direction.normalize();
                (*start + dir * link_length * (i as f32 + 0.5), dir)
            }
            ChainPath::Curve { sample, .. } => {
                let tangent = (sample((t + eps).min(1.0)) - sample((t - eps).max(0.0))).normalize();
                (sample(t), tangent)
            }
        };

        let rotation = Quat::from_rotation_arc(Vec3::Y, tangent);

        let mut entity = commands.spawn((
            collider.clone(),
            RigidBody::Dynamic,
            Transform::from_translation(center).with_rotation(rotation),
        ));

        if let Some((parent_entity, parent_center, parent_rotation)) = prev {
            let (parent_local, child_local) = if i == 0 && def.anchored {
                // el anchor fija su propio centro; el eslabón conecta por su extremo trasero
                (Vec3::ZERO, rotation.inverse() * (anchor_point - center))
            } else {
                // el punto de conexión es el midpoint entre centros adyacentes,
                // expresado en espacio local de cada body
                let connection = (parent_center + center) / 2.0;
                (parent_rotation.inverse() * (connection - parent_center),
                 rotation.inverse() * (connection - center))
            };

            entity.insert(match &def.path {
                ChainPath::Linear { .. } => ImpulseJoint::new(parent_entity,
                    RevoluteJointBuilder::new(hinge_axis)
                        .local_anchor1(parent_local)
                        .local_anchor2(child_local)
                        .build()),
                ChainPath::Curve { .. } => ImpulseJoint::new(parent_entity,
                    SphericalJointBuilder::new()
                        .local_anchor1(parent_local)
                        .local_anchor2(child_local)
                        .build()),
            });
        }

        prev = Some((entity.id(), center, rotation));
    }
}
