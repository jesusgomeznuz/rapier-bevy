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

// Relación longitud_eslabón / radio_eslabón. Calibrado con los 3 ejemplos base
// (vertical 10seg, horizontal 8seg, arco 12seg). Bajar = más densidad, subir = menos.
const LINK_LENGTH_TO_RADIUS: f32 = 5.0;

pub enum ChainPath {
    Linear { start: Vec3, direction: Vec3, length: f32 },
    Curve  { sample: Box<dyn Fn(f32) -> Vec3>, length: f32 },
}

pub struct ChainDef {
    pub path:             ChainPath,
    pub radius:           f32,
    pub anchored:         bool,
    pub angular_damping:  f32,
    pub linear_damping:   f32,
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

    let link_length = def.radius * LINK_LENGTH_TO_RADIUS;
    let segments    = (total_length / link_length).ceil() as u32;
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

    for i in 0..segments {
        let t   = (i as f32 + 0.5) / segments as f32;
        let eps = 0.5 / segments as f32;

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
            Damping { angular_damping: def.angular_damping, linear_damping: def.linear_damping },
        ));

        if let Some((parent_entity, parent_center, parent_rotation)) = prev {
            let (parent_local, child_local) = if i == 0 && def.anchored {
                // el anchor fija su propio centro; el eslabón conecta por su extremo trasero
                (Vec3::ZERO, rotation.inverse() * (anchor_point - center))
            } else {
                // el punto de conexión es el midpoint entre las puntas reales de las cápsulas
                // (no entre centros) — para curvas donde los eslabones tienen distinta orientación,
                // usar centros pone el anchor dentro de ambos cuerpos causando traslape visual
                let parent_tip  = parent_center + parent_rotation * Vec3::new(0.0,  link_half, 0.0);
                let child_tail  = center        + rotation        * Vec3::new(0.0, -link_half, 0.0);
                let connection  = (parent_tip + child_tail) / 2.0;
                (parent_rotation.inverse() * (connection - parent_center),
                 rotation.inverse() * (connection - center))
            };

            entity.insert(match &def.path {
                ChainPath::Linear { .. } => ImpulseJoint::new(parent_entity,
                    RevoluteJointBuilder::new(hinge_axis)
                        .local_anchor1(parent_local)
                        .local_anchor2(child_local)
                        .build()),
                ChainPath::Curve { .. } => {
                    // eje de bisagra calculado por eslabón desde la tangente local —
                    // previene el giro libre sobre el eje del eslabón (bug con SphericalJoint)
                    let world_axis = {
                        let perp = tangent.cross(Vec3::Y);
                        if perp.length_squared() > 0.001 { perp.normalize() } else { Vec3::Z }
                    };
                    let local_axis = parent_rotation.inverse() * world_axis;
                    ImpulseJoint::new(parent_entity,
                        RevoluteJointBuilder::new(local_axis)
                            .local_anchor1(parent_local)
                            .local_anchor2(child_local)
                            .build())
                },
            });
        }

        prev = Some((entity.id(), center, rotation));
    }
}
