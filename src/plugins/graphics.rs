use bevy::prelude::*;
use bevy_rapier3d::render::RapierDebugRenderPlugin;
use crate::modes::debug_enabled;

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        if debug_enabled() {
            app.add_plugins(RapierDebugRenderPlugin::default());
        }
        app.add_systems(Startup, spawn_scene_lights);
    }
}

fn spawn_scene_lights(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 4.5, 12.0).looking_at(Vec3::new(3.5, 0.8, 0.0), Vec3::Y),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 80.0,
        ..default()
    });

    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}
