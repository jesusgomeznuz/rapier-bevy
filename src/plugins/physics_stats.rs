use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

pub struct PhysicsStatsPlugin;

impl Plugin for PhysicsStatsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_physics_stats)
            .add_systems(Update, update_physics_stats);
    }
}

#[derive(Component)]
struct PhysicsStatsOverlay;

fn spawn_physics_stats(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        PhysicsStatsOverlay,
    ));
}

fn update_physics_stats(
    diagnostics: Res<DiagnosticsStore>,
    bodies: Query<&RigidBody>,
    joints: Query<&ImpulseJoint>,
    mut overlay: Query<&mut Text, With<PhysicsStatsOverlay>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let n_dynamic = bodies.iter().filter(|rb| matches!(rb, RigidBody::Dynamic)).count();
    let n_joints = joints.iter().count();

    if let Ok(mut text) = overlay.single_mut() {
        **text = format!("{fps:.0} fps\nbodies  {n_dynamic}\njoints  {n_joints}");
    }
}
