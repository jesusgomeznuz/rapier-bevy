use crate::modes::SimMode;
use bevy::prelude::*;
use bevy_rapier3d::{
    parry::{
        math::{Isometry, Point},
        shape::SharedShape,
    },
    prelude::*,
};

#[allow(dead_code)]
pub enum ColliderShape {
    Box { hx: f32, hy: f32, hz: f32 },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
    MeshObject { model_name: &'static str },
}

pub enum BodyType {
    Static,
    Dynamic,
}

pub struct ObjectDef {
    pub shape: ColliderShape,
    pub position: Vec3,
    pub body: BodyType,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
}

impl Default for ObjectDef {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Box { hx: 1.0, hy: 1.0, hz: 1.0 },
            position: Vec3::ZERO,
            body: BodyType::Static,
            friction: None,
            restitution: None,
        }
    }
}

pub fn preprocess_assets() {
    let start = std::time::Instant::now();
    preprocess_obj("assets/half_ring.obj", "assets/half_ring.compound");
    preprocess_obj("assets/gear.obj",      "assets/gear.compound");
    println!("[preprocess] total: {:.2?}", start.elapsed());
}

pub fn spawn_object(commands: &mut Commands, def: ObjectDef, mode: &SimMode) {
    let mut entity = commands.spawn((
        build_collider(def.shape, mode),
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
}

pub fn preprocess_obj(obj_path: &str, output_path: &str) {
    println!("Preprocessing {}...", obj_path);
    let start = std::time::Instant::now();

    let (vertices, indices) = load_obj(obj_path);
    let shape = SharedShape::convex_decomposition_with_params(&vertices, &indices, &vhacd_params());

    let parts: Vec<Vec<[f32; 3]>> = shape
        .as_compound()
        .expect("VHACD should produce a compound shape")
        .shapes()
        .iter()
        .filter_map(|(_, s)| s.as_convex_polyhedron())
        .map(|c| c.points().iter().map(|p| [p.x, p.y, p.z]).collect())
        .collect();

    let data = bincode::serialize(&parts).expect("Failed to serialize");
    std::fs::write(output_path, &data)
        .unwrap_or_else(|_| panic!("Failed to write {}", output_path));

    println!("  -> {} convex pieces in {:.2?}", parts.len(), start.elapsed());
}

fn build_collider(shape: ColliderShape, mode: &SimMode) -> Collider {
    match shape {
        ColliderShape::Box { hx, hy, hz }          => Collider::cuboid(hx, hy, hz),
        ColliderShape::Sphere { radius }            => Collider::ball(radius),
        ColliderShape::Capsule { half_height, radius } => Collider::capsule_y(half_height, radius),
        ColliderShape::MeshObject { model_name }   => match mode {
            SimMode::Precomputed => load_compound(model_name),
            SimMode::Raw         => decompose_obj(&format!("assets/{}.obj", model_name)),
        },
    }
}

fn load_obj(path: &str) -> (Vec<Point<f32>>, Vec<[u32; 3]>) {
    let (models, _) = tobj::load_obj(path, &tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    })
    .unwrap_or_else(|_| panic!("Failed to load obj: {}", path));

    let mesh = &models[0].mesh;
    let vertices = mesh.positions.chunks_exact(3)
        .map(|xyz| Point::from([xyz[0], xyz[1], xyz[2]]))
        .collect();
    let indices = mesh.indices.chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect();

    (vertices, indices)
}

fn vhacd_params() -> VHACDParameters {
    VHACDParameters { resolution: 128, ..Default::default() }
}

fn decompose_obj(obj_path: &str) -> Collider {
    let (vertices, indices) = load_obj(obj_path);
    Collider::from(SharedShape::convex_decomposition_with_params(
        &vertices,
        &indices,
        &vhacd_params(),
    ))
}

fn load_compound(model_name: &str) -> Collider {
    let path = format!("assets/{}.compound", model_name);
    let data = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("Compound file not found: {}", path));

    let parts: Vec<Vec<[f32; 3]>> =
        bincode::deserialize(&data).expect("Failed to deserialize compound");

    let shapes: Vec<_> = parts
        .into_iter()
        .filter_map(|pts| {
            let points: Vec<Point<f32>> = pts.iter().map(|p| Point::from(*p)).collect();
            SharedShape::convex_hull(&points)
        })
        .map(|s| (Isometry::identity(), s))
        .collect();

    Collider::from(SharedShape::compound(shapes))
}
