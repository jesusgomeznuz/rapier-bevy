pub(crate) mod colliders;
mod staircase;
mod vehicle;
mod chain;
mod bench;

pub use bench::{spawn_chain_grid, spawn_falling_spheres, spawn_stacked_boxes};
pub use colliders::preprocess_obj;
pub use chain::{ChainDef, ChainPath, spawn_chain};
pub use staircase::spawn_staircase;
pub use vehicle::{VehicleDef, spawn_vehicle};

use crate::modes::SimMode;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy_mod_rounded_box::RoundedBox;
use bevy_rapier3d::prelude::*;

#[allow(dead_code)]
#[derive(Clone)]
pub enum ColliderShape {
    Box { hx: f32, hy: f32, hz: f32 },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
    Cylinder { half_height: f32, radius: f32, axis: Vec3 },
    MeshObject { model_name: String },
}

#[allow(dead_code)]
pub enum BodyType {
    Static,
    Dynamic,
    Kinematic,
}

pub struct MotorDef {
    pub target_velocity: f32,
    pub damping: f32,
}

pub enum JointDef {
    Revolute {
        parent: Entity,
        axis: Vec3,
        parent_anchor: Vec3,
        self_anchor: Vec3,
        motor: Option<MotorDef>,
    },
}

pub enum VisualAppearance {
    Color(Color),
    Texture(&'static str),
    Scene(&'static str),
}

impl Default for VisualAppearance {
    fn default() -> Self {
        VisualAppearance::Color(Color::srgb(0.7, 0.7, 0.7))
    }
}

pub struct VisualDef {
    pub appearance: VisualAppearance,
    pub metallic: f32,
    pub roughness: f32,
    pub border_radius: Option<f32>,
}

impl Default for VisualDef {
    fn default() -> Self {
        Self { appearance: VisualAppearance::default(), metallic: 0.0, roughness: 0.8, border_radius: None }
    }
}

impl VisualDef {
    pub fn white_matte() -> Self {
        Self { appearance: VisualAppearance::Color(Color::srgb(0.92, 0.92, 0.90)), roughness: 0.85, ..default() }
    }
    pub fn black_matte() -> Self {
        Self { appearance: VisualAppearance::Color(Color::srgb(0.08, 0.08, 0.10)), roughness: 0.50, ..default() }
    }
    pub fn gold() -> Self {
        Self { appearance: VisualAppearance::Color(Color::srgb(0.90, 0.70, 0.10)), metallic: 0.2, roughness: 0.3, ..default() }
    }
    pub fn steel() -> Self {
        Self { appearance: VisualAppearance::Color(Color::srgb(0.65, 0.65, 0.68)), metallic: 0.95, roughness: 0.25, ..default() }
    }
    pub fn grass_green() -> Self {
        Self { appearance: VisualAppearance::Color(Color::srgb(0.22, 0.38, 0.22)), roughness: 0.90, ..default() }
    }
    pub fn from_texture(path: &'static str) -> Self {
        Self { appearance: VisualAppearance::Texture(path), ..default() }
    }
    pub fn from_scene(path: &'static str) -> Self {
        Self { appearance: VisualAppearance::Scene(path), ..default() }
    }
}

pub struct ObjectDef {
    pub shape: ColliderShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub body: BodyType,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    pub angular_damping: Option<f32>,
    pub linear_damping: Option<f32>,
    pub locked_axes: Option<LockedAxes>,
    pub collision_groups: Option<CollisionGroups>,
    pub joint: Option<JointDef>,
    pub velocity: Option<Vec3>,
    pub angvel: Option<Vec3>,
    pub visual: Option<VisualDef>,
    pub ccd: bool,
    pub sensor: bool,
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
            linear_damping: None,
            locked_axes: None,
            collision_groups: None,
            joint: None,
            velocity: None,
            angvel: None,
            visual: None,
            ccd: false,
            sensor: false,
        }
    }
}

pub fn spawn_object(
    commands: &mut Commands,
    def: ObjectDef,
    mode: &SimMode,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let scene_handle = def.visual.as_ref().and_then(|vis| {
        if let VisualAppearance::Scene(path) = &vis.appearance {
            Some(asset_server.load::<bevy::scene::Scene>(*path))
        } else {
            None
        }
    });

    let visual_child_rotation = match &def.shape {
        ColliderShape::Cylinder { axis, .. } => Quat::from_rotation_arc(Vec3::Y, *axis),
        _ => Quat::IDENTITY,
    };

    let mesh_visual = def.visual.as_ref().and_then(|vis| {
        let mat = match &vis.appearance {
            VisualAppearance::Color(color) => Some(StandardMaterial {
                base_color: *color,
                metallic: vis.metallic,
                perceptual_roughness: vis.roughness,
                ..default()
            }),
            VisualAppearance::Texture(path) => Some(StandardMaterial {
                base_color_texture: Some(asset_server.load(*path)),
                metallic: vis.metallic,
                perceptual_roughness: vis.roughness,
                ..default()
            }),
            VisualAppearance::Scene(_) => None,
        };
        mat.and_then(|m| {
            let mesh_handle = match &def.shape {
                ColliderShape::MeshObject { model_name } =>
                    Some(meshes.add(colliders::build_mesh_from_obj(model_name))),
                shape => build_mesh_for_shape(shape, vis.border_radius).map(|mesh| meshes.add(mesh)),
            }?;
            Some((mesh_handle, materials.add(m)))
        })
    });

    let collider_border = def.visual.as_ref().and_then(|v| v.border_radius);
    let mut entity = commands.spawn((
        colliders::build_collider(def.shape, collider_border, mode),
        Transform::from_translation(def.position).with_rotation(def.rotation),
    ));

    match def.body {
        BodyType::Dynamic   => { entity.insert((RigidBody::Dynamic, Velocity::default())); }
        BodyType::Kinematic => { entity.insert(RigidBody::KinematicVelocityBased); }
        BodyType::Static    => {}
    }
    if let Some(f) = def.friction {
        entity.insert(Friction::coefficient(f));
    }
    if let Some(r) = def.restitution {
        entity.insert(Restitution::coefficient(r));
    }
    if def.angular_damping.is_some() || def.linear_damping.is_some() {
        entity.insert(Damping {
            angular_damping: def.angular_damping.unwrap_or(0.0),
            linear_damping:  def.linear_damping.unwrap_or(0.0),
        });
    }
    if let Some(axes) = def.locked_axes {
        entity.insert(axes);
    }
    if def.ccd {
        entity.insert(Ccd::enabled());
    }
    if def.sensor {
        entity.insert((Sensor, ActiveEvents::COLLISION_EVENTS));
    }
    if def.velocity.is_some() || def.angvel.is_some() {
        entity.insert(Velocity {
            linvel: def.velocity.unwrap_or(Vec3::ZERO),
            angvel: def.angvel.unwrap_or(Vec3::ZERO),
        });
    }
    if let Some(groups) = def.collision_groups {
        entity.insert(groups);
    }
    if let Some(joint_def) = def.joint {
        match joint_def {
            JointDef::Revolute { parent, axis, parent_anchor, self_anchor, motor } => {
                let mut builder = RevoluteJointBuilder::new(axis)
                    .local_anchor1(parent_anchor)
                    .local_anchor2(self_anchor);
                if let Some(m) = motor {
                    builder = builder.motor_velocity(m.target_velocity, m.damping);
                }
                entity.insert(ImpulseJoint::new(parent, builder.build()));
            }
        }
    }
    if let Some(handle) = scene_handle {
        entity.insert(Visibility::default());
        entity.with_children(|parent| { parent.spawn(SceneRoot(handle)); });
    }
    if let Some((mesh_handle, mat_handle)) = mesh_visual {
        entity.insert(Visibility::default());
        entity.with_children(|parent| {
            parent.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat_handle),
                Transform::from_rotation(visual_child_rotation),
            ));
        });
    }

    entity.id()
}

fn build_mesh_for_shape(shape: &ColliderShape, border_radius: Option<f32>) -> Option<Mesh> {
    match shape {
        ColliderShape::Box { hx, hy, hz } => {
            if let Some(radius) = border_radius {
                Some(RoundedBox { size: Vec3::new(hx * 2.0, hy * 2.0, hz * 2.0), radius, ..default() }.into())
            } else {
                Some(Cuboid::new(hx * 2.0, hy * 2.0, hz * 2.0).into())
            }
        }
        ColliderShape::Sphere { radius }                => Some(Sphere::new(*radius).into()),
        ColliderShape::Capsule { half_height, radius }  => Some(Capsule3d::new(*radius, half_height * 2.0).into()),
        ColliderShape::Cylinder { half_height, radius, .. } => Some(match border_radius {
            Some(br) if br > 0.0 => build_round_cylinder_mesh(*half_height, *radius, br),
            _ => Cylinder::new(*radius, half_height * 2.0).into(),
        }),
        ColliderShape::MeshObject { .. }                => None,
    }
}

fn build_round_cylinder_mesh(half_height: f32, radius: f32, border: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::mesh::{Indices, PrimitiveTopology};

    let n_radial = 48;
    let n_arc    = 8;
    let h_inner  = (half_height - border).max(0.0);
    let r_inner  = (radius - border).max(0.0);

    let mut profile: Vec<(f32, f32)> = Vec::new();
    profile.push((0.0, half_height));
    profile.push((r_inner, half_height));
    for i in 1..n_arc {
        let theta = std::f32::consts::FRAC_PI_2 * (i as f32) / (n_arc as f32);
        let (s, c) = theta.sin_cos();
        profile.push((r_inner + border * s, h_inner + border * c));
    }
    profile.push((radius, h_inner));
    profile.push((radius, -h_inner));
    for i in 1..n_arc {
        let theta = std::f32::consts::FRAC_PI_2 * (i as f32) / (n_arc as f32);
        let (s, c) = theta.sin_cos();
        profile.push((r_inner + border * c, -h_inner - border * s));
    }
    profile.push((r_inner, -half_height));
    profile.push((0.0, -half_height));

    let mut positions: Vec<[f32; 3]> = Vec::new();
    for &(pr, py) in &profile {
        for j in 0..n_radial {
            let phi = std::f32::consts::TAU * (j as f32) / (n_radial as f32);
            let (s, c) = phi.sin_cos();
            positions.push([pr * c, py, pr * s]);
        }
    }

    let mut indices: Vec<u32> = Vec::new();
    for i in 0..(profile.len() - 1) as u32 {
        for j in 0..n_radial as u32 {
            let jn = (j + 1) % n_radial as u32;
            let a = i       * n_radial as u32 + j;
            let b = (i + 1) * n_radial as u32 + j;
            let c = (i + 1) * n_radial as u32 + jn;
            let d = i       * n_radial as u32 + jn;
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_normals();
    mesh
}

pub fn preprocess_assets() {
    use bevy_rapier3d::prelude::VHACDParameters;
    let start = std::time::Instant::now();
    colliders::preprocess_obj(
        "assets/vehicle-racer.obj",
        "assets/vehicle-racer-chassis.compound",
        Some("vehicle-racer"),
        VHACDParameters { resolution: 64, ..Default::default() },
    );
    colliders::preprocess_obj(
        "assets/wheel-medium.obj",
        "assets/wheel-medium.compound",
        None,
        VHACDParameters { resolution: 64, ..Default::default() },
    );
    println!("[preprocess] total: {:.2?}", start.elapsed());
}
