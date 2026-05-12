use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::render::RapierDebugRenderPlugin;

use crate::modes::{SimMode, debug_enabled, record_duration};
use crate::plugins::{PhysicsStatsPlugin, RecordPlugin};

pub struct GameAppConfig {
    pub title: &'static str,
    pub resolution: (f32, f32),
}

impl Default for GameAppConfig {
    fn default() -> Self {
        Self {
            title: "rapier-bevy",
            resolution: (1280.0, 720.0),
        }
    }
}

pub fn game_app(mode: SimMode, config: GameAppConfig) -> App {
    let mut app = App::new();

    match record_duration() {
        None => add_windowed_plugins(&mut app, &config),
        Some(secs) => {
            app.add_plugins(RecordPlugin { duration_secs: secs });
        }
    }

    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());

    if debug_enabled() {
        app.add_plugins(RapierDebugRenderPlugin::default());
    }

    app.insert_resource(mode);
    app
}

fn add_windowed_plugins(app: &mut App, config: &GameAppConfig) {
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            resolution: config.resolution.into(),
            title: config.title.into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(FrameTimeDiagnosticsPlugin::default())
    .add_plugins(PhysicsStatsPlugin);
}
