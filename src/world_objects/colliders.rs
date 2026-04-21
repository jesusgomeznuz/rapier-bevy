use bevy_rapier3d::{
    parry::{
        math::{Isometry, Point},
        shape::SharedShape,
    },
    prelude::*,
};
use crate::modes::SimMode;
use super::ColliderShape;

pub fn build_collider(shape: ColliderShape, mode: &SimMode) -> Collider {
    match shape {
        ColliderShape::Box { hx, hy, hz }             => Collider::cuboid(hx, hy, hz),
        ColliderShape::Sphere { radius }               => Collider::ball(radius),
        ColliderShape::Capsule { half_height, radius } => Collider::capsule_y(half_height, radius),
        ColliderShape::MeshObject { model_name }       => match mode {
            SimMode::Precomputed       => load_compound(model_name),
            SimMode::Raw | SimMode::Bench { .. } => {
                let (obj_name, group) = obj_source(model_name);
                decompose_obj(&format!("assets/{}.obj", obj_name), group)
            }
        },
    }
}

pub fn preprocess_obj(obj_path: &str, output_path: &str, group: Option<&str>, params: VHACDParameters) {
    println!("Preprocessing {} (group={:?})...", obj_path, group);
    let start = std::time::Instant::now();

    let (vertices, indices) = load_obj(obj_path, group);
    let shape = SharedShape::convex_decomposition_with_params(&vertices, &indices, &params);

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

fn load_obj(path: &str, group: Option<&str>) -> (Vec<Point<f32>>, Vec<[u32; 3]>) {
    let (models, _) = tobj::load_obj(path, &tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    })
    .unwrap_or_else(|_| panic!("Failed to load obj: {}", path));

    let selected: Vec<_> = match group {
        Some(name) => models.iter().filter(|m| m.name == name).collect(),
        None       => models.iter().collect(),
    };

    let mut vertices = vec![];
    let mut indices  = vec![];
    let mut offset   = 0u32;

    for model in selected {
        let mesh = &model.mesh;
        let n = (mesh.positions.len() / 3) as u32;
        vertices.extend(mesh.positions.chunks_exact(3)
            .map(|xyz| Point::from([xyz[0], xyz[1], xyz[2]])));
        indices.extend(mesh.indices.chunks_exact(3)
            .map(|tri| [tri[0] + offset, tri[1] + offset, tri[2] + offset]));
        offset += n;
    }

    (vertices, indices)
}

fn chassis_params() -> VHACDParameters {
    VHACDParameters { resolution: 64, max_convex_hulls: 4, ..Default::default() }
}

fn wheel_params() -> VHACDParameters {
    VHACDParameters { resolution: 256, max_convex_hulls: 3, concavity: 0.0001, ..Default::default() }
}

// Mapea el nombre del asset al OBJ fuente y al grupo a filtrar
fn obj_source(model_name: &str) -> (&str, Option<&str>) {
    match model_name {
        "vehicle-racer-chassis" => ("vehicle-racer", Some("vehicle-racer")),
        other => (other, None),
    }
}

fn decompose_obj(obj_path: &str, group: Option<&str>) -> Collider {
    let (vertices, indices) = load_obj(obj_path, group);
    Collider::from(SharedShape::convex_decomposition_with_params(
        &vertices,
        &indices,
        &chassis_params(),
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
