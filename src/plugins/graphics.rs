use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy_rapier3d::render::RapierDebugRenderPlugin;
use crate::modes::debug_enabled;
use crate::plugins::record::OffscreenTarget;

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        if debug_enabled() {
            app.add_plugins(RapierDebugRenderPlugin::default());
        }
        app.add_systems(Startup, spawn_camera_and_lights);
    }
}

fn spawn_camera_and_lights(
    mut commands: Commands,
    offscreen: Option<Res<OffscreenTarget>>,
) {
    let camera_target = offscreen.map(|o| Camera {
        target: RenderTarget::Image(o.image.clone().into()),
        ..default()
    });

    if let Some(camera) = camera_target {
        commands.spawn((
            Camera3d::default(),
            camera,
            Transform::from_xyz(0.0, 13.0, 22.0).looking_at(Vec3::new(0.0, 12.0, 0.0), Vec3::Y),
        ));
    } else {
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 13.0, 22.0).looking_at(Vec3::new(0.0, 12.0, 0.0), Vec3::Y),
        ));
    }

    commands.insert_resource(AmbientLight {
        color:      Color::WHITE,
        brightness: 80.0,
        ..default()
    });

    commands.spawn((
        DirectionalLight {
            illuminance:      8000.0,
            shadows_enabled:  true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}
