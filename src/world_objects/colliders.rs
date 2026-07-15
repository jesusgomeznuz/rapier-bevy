use bevy::asset::RenderAssetUsages;
use bevy::prelude::{Mesh, Quat, Vec3};
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy_rapier3d::{
    parry::{
        math::{Isometry, Point},
        shape::SharedShape,
    },
    prelude::*,
};
use super::ColliderShape;

pub fn build_collider(shape: ColliderShape, border_radius: Option<f32>) -> Collider {
    match shape {
        ColliderShape::Box { hx, hy, hz } => match border_radius {
            Some(br) if br > 0.0 => Collider::round_cuboid(
                (hx - br).max(0.001),
                (hy - br).max(0.001),
                (hz - br).max(0.001),
                br,
            ),
            _ => Collider::cuboid(hx, hy, hz),
        },
        ColliderShape::Sphere { radius }               => Collider::ball(radius),
        ColliderShape::Capsule { half_height, radius } => Collider::capsule_y(half_height, radius),
        ColliderShape::Cylinder { half_height, radius, axis } => {
            let inner = match border_radius {
                Some(br) if br > 0.0 => Collider::round_cylinder(
                    (half_height - br).max(0.001),
                    (radius - br).max(0.001),
                    br,
                ),
                _ => Collider::cylinder(half_height, radius),
            };
            Collider::compound(vec![(
                Vec3::ZERO,
                Quat::from_rotation_arc(Vec3::Y, axis),
                inner,
            )])
        }
        // Los colisionadores de malla SIEMPRE vienen precomputados (.compound
        // VHACD): quien fabrica el asset fabrica su compound (torus_assets,
        // preprocess_obj). El juego nunca descompone geometría en runtime.
        ColliderShape::MeshObject { model_name } => load_compound(&model_name),
    }
}

pub fn build_mesh_from_obj(model_name: &str) -> Mesh {
    let (obj_name, group) = obj_source(model_name);
    let path = format!("assets/{}.obj", obj_name);
    let (models, _) = tobj::load_obj(&path, &tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    })
    .unwrap_or_else(|_| panic!("Failed to load obj: {}", path));

    let selected: Vec<_> = match group {
        Some(name) => models.iter().filter(|m| m.name == name).collect(),
        None       => models.iter().collect(),
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices:   Vec<u32>      = Vec::new();
    let mut offset    = 0u32;

    for model in selected {
        let mesh = &model.mesh;
        let n = (mesh.positions.len() / 3) as u32;
        positions.extend(mesh.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]));
        indices.extend(mesh.indices.iter().map(|i| i + offset));
        offset += n;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_normals();
    mesh
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

    let json_path = output_path.replace(".compound", ".compound.json");
    std::fs::write(&json_path, parts_to_json(&parts))
        .unwrap_or_else(|_| panic!("Failed to write {}", json_path));

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

// Mapea el nombre del asset al OBJ fuente y al grupo a filtrar
fn obj_source(model_name: &str) -> (&str, Option<&str>) {
    match model_name {
        "vehicle-racer-chassis" => ("vehicle-racer", Some("vehicle-racer")),
        other => (other, None),
    }
}

fn parts_to_json(parts: &Vec<Vec<[f32; 3]>>) -> String {
    let hulls: Vec<String> = parts.iter().map(|hull| {
        let pts: Vec<String> = hull.iter()
            .map(|p| format!("[{},{},{}]", p[0], p[1], p[2]))
            .collect();
        format!("[{}]", pts.join(","))
    }).collect();
    format!("[{}]", hulls.join(","))
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
