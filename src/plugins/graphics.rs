use crate::modes::debug_enabled;
use crate::plugins::record::OffscreenTarget;
use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy_rapier3d::render::RapierDebugRenderPlugin;

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        if debug_enabled() {
            app.add_plugins(RapierDebugRenderPlugin::default());
        }
        app.add_systems(Startup, spawn_camera_and_lights);
    }
}

fn spawn_camera_and_lights(mut commands: Commands, offscreen: Option<Res<OffscreenTarget>>) {
    let render_target: Option<RenderTarget> = offscreen
        .as_ref()
        .map(|o| RenderTarget::Image(o.image.clone().into()));

    let mut cam3d = Camera::default();
    if let Some(ref t) = render_target {
        cam3d.target = t.clone();
    }
    commands
        .spawn((
            Camera3d::default(),
            cam3d,
            Transform::from_xyz(0.0, 13.0, 22.0).looking_at(Vec3::new(0.0, 12.0, 0.0), Vec3::Y),
        ))
        .with_children(|camera| {
            camera.spawn((
                DirectionalLight {
                    illuminance: 8_000.0,
                    shadows_enabled: true,
                    ..default()
                },
                Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.2, 0.0, 0.0)),
            ));
        });

    // Camera2d renderiza Text2d (nicknames) encima de la escena 3D.
    // clear_color::None evita que borre el frame 3D ya renderizado.
    let mut cam2d = Camera {
        order: 1,
        clear_color: ClearColorConfig::None,
        ..default()
    };
    if let Some(ref t) = render_target {
        cam2d.target = t.clone();
    }
    commands.spawn((Camera2d, cam2d));

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 120.0,
        ..default()
    });
}
