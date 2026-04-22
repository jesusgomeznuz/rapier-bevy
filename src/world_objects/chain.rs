use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

pub struct ChainDef {
    pub path: ChainPath,
    pub radius: f32,
    pub anchored: bool,
    pub angular_damping: f32,
    pub linear_damping: f32,
}

pub enum ChainPath {
    Linear {
        start: Vec3,
        direction: Vec3,
        length: f32,
    },
    Curve {
        sample: Box<dyn Fn(f32) -> Vec3>,
        length: f32,
    },
}

pub fn spawn_chain(
    commands: &mut Commands,
    def: ChainDef,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // calibrado con vertical 10seg, horizontal 8seg, arco 12seg
    let link_length_to_radius: f32 = 5.0;

    let (total_length, anchor_point) = match &def.path {
        ChainPath::Linear { length, start, .. } => (*length, *start),
        ChainPath::Curve { length, sample } => (*length, sample(0.0)),
    };

    let link_length = def.radius * link_length_to_radius;
    let segments = (total_length / link_length).ceil() as u32;
    let link_half = link_length / 2.0;
    let half_height = (link_half - def.radius).max(0.001);

    let collider = Collider::capsule_y(half_height, def.radius);
    let link_mesh = meshes.add(Capsule3d::new(def.radius, link_half));
    let link_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.65, 0.65, 0.68),
        metallic: 0.95,
        perceptual_roughness: 0.25,
        ..default()
    });

    let hinge_axis = linear_hinge_axis(&def.path);

    let mut prev: Option<(Entity, Vec3, Quat)> = if def.anchored {
        let e = commands
            .spawn((RigidBody::Fixed, Transform::from_translation(anchor_point)))
            .id();
        Some((e, anchor_point, Quat::IDENTITY))
    } else {
        None
    };

    for i in 0..segments {
        let t = (i as f32 + 0.5) / segments as f32;
        let eps = 0.5 / segments as f32;

        let (center, tangent) = sample_path(&def.path, t, eps, i, link_length);
        let rotation = Quat::from_rotation_arc(Vec3::Y, tangent);

        let mut entity = commands.spawn((
            collider.clone(),
            RigidBody::Dynamic,
            Transform::from_translation(center).with_rotation(rotation),
            Damping {
                angular_damping: def.angular_damping,
                linear_damping: def.linear_damping,
            },
            Visibility::default(),
        ));

        let (mesh_h, mat_h) = (link_mesh.clone(), link_material.clone());
        entity.with_children(|parent| {
            parent.spawn((
                Mesh3d(mesh_h),
                MeshMaterial3d(mat_h),
                Transform::from_scale(Vec3::new(1.0, 1.5, 1.0)),
            ));
        });

        if let Some((parent_entity, parent_center, parent_rotation)) = prev {
            let (parent_local, child_local) = link_anchors(
                i, def.anchored, anchor_point, center, rotation,
                parent_center, parent_rotation, link_half,
            );

            entity.insert(link_joint(
                &def.path, parent_entity, hinge_axis, tangent,
                parent_rotation, parent_local, child_local,
            ));
        }

        prev = Some((entity.id(), center, rotation));
    }
}

fn linear_hinge_axis(path: &ChainPath) -> Vec3 {
    match path {
        ChainPath::Linear { direction, .. } => {
            let dir = direction.normalize();
            let perp = dir.cross(Vec3::Y);
            if perp.length_squared() > 0.001 { perp.normalize() } else { Vec3::Z }
        }
        ChainPath::Curve { .. } => Vec3::Z,
    }
}

fn sample_path(path: &ChainPath, t: f32, eps: f32, i: u32, link_length: f32) -> (Vec3, Vec3) {
    match path {
        ChainPath::Linear { start, direction, .. } => {
            let dir = direction.normalize();
            (*start + dir * link_length * (i as f32 + 0.5), dir)
        }
        ChainPath::Curve { sample, .. } => {
            let tangent = (sample((t + eps).min(1.0)) - sample((t - eps).max(0.0))).normalize();
            (sample(t), tangent)
        }
    }
}

fn link_anchors(
    i: u32,
    anchored: bool,
    anchor_point: Vec3,
    center: Vec3,
    rotation: Quat,
    parent_center: Vec3,
    parent_rotation: Quat,
    link_half: f32,
) -> (Vec3, Vec3) {
    if i == 0 && anchored {
        (Vec3::ZERO, rotation.inverse() * (anchor_point - center))
    } else {
        // midpoint entre puntas reales (no centros) — evita traslape visual en curvas
        let parent_tip = parent_center + parent_rotation * Vec3::new(0.0, link_half, 0.0);
        let child_tail = center + rotation * Vec3::new(0.0, -link_half, 0.0);
        let connection = (parent_tip + child_tail) / 2.0;
        (
            parent_rotation.inverse() * (connection - parent_center),
            rotation.inverse() * (connection - center),
        )
    }
}

fn link_joint(
    path: &ChainPath,
    parent: Entity,
    hinge_axis: Vec3,
    tangent: Vec3,
    parent_rotation: Quat,
    parent_local: Vec3,
    child_local: Vec3,
) -> ImpulseJoint {
    match path {
        ChainPath::Linear { .. } => ImpulseJoint::new(
            parent,
            RevoluteJointBuilder::new(hinge_axis)
                .local_anchor1(parent_local)
                .local_anchor2(child_local)
                .build(),
        ),
        ChainPath::Curve { .. } => {
            // eje por eslabón desde tangente×Y — previene giro libre sobre el eje del eslabón
            let world_axis = {
                let perp = tangent.cross(Vec3::Y);
                if perp.length_squared() > 0.001 { perp.normalize() } else { Vec3::Z }
            };
            let local_axis = parent_rotation.inverse() * world_axis;
            ImpulseJoint::new(
                parent,
                RevoluteJointBuilder::new(local_axis)
                    .local_anchor1(parent_local)
                    .local_anchor2(child_local)
                    .build(),
            )
        }
    }
}
