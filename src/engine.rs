use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::render::RapierDebugRenderPlugin;
use std::time::Duration;

use crate::modes::{SimulationMode, debug_enabled, play_timeline, record_duration, write_timeline_duration};
use crate::plugins::{PhysicsStatsPlugin, PlayPlugin, RecordPlugin, WriteTimelinePlugin};

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

pub fn random_physics_game_app(mode: SimulationMode, config: GameAppConfig) -> App {
    let mut app = App::new();

    let writing_timeline = write_timeline_duration();
    match (writing_timeline, record_duration()) {
        (Some(_), _)       => add_headless_plugins(&mut app),
        (None, Some(secs)) => { app.add_plugins(RecordPlugin { duration_secs: secs }); }
        (None, None)       => add_windowed_plugins(&mut app, &config),
    }

    // Toda la simulación corre en FixedUpdate a 60 steps/s. Ese es el reloj de verdad
    // del juego; los modificadores --record y --write-timeline aceleran el wall-clock para
    // generar simulaciones largas rápido, sin alterar la duración lógica de nada.
    app.insert_resource(Time::<Fixed>::from_hz(60.0));

    match (writing_timeline, play_timeline()) {
        // Escribir timeline: física + captura; gana sobre --record/--play.
        (Some(secs), _) => {
            add_physics(&mut app);
            app.add_plugins(WriteTimelinePlugin { duration_secs: secs });
        }
        // Play: SIN física — la timeline dicta los Transforms y Bevy solo dibuja.
        // Combina con --record (video) o con ventana (preview).
        (None, Some(path)) => {
            app.add_plugins(PlayPlugin { path });
        }
        (None, None) => {
            add_physics(&mut app);
            if debug_enabled() {
                app.add_plugins(RapierDebugRenderPlugin::default());
            }
        }
    }

    app.insert_resource(mode);
    app
}

pub fn deterministic_physics_game_app(config: GameAppConfig) -> App {
    let mut app = App::new();

    match record_duration() {
        Some(secs) => { app.add_plugins(RecordPlugin { duration_secs: secs }); }
        None => add_windowed_plugins(&mut app, &config),
    }

    app.insert_resource(Time::<Fixed>::from_hz(60.0));
    app
}

// El TimestepMode va antes del plugin para que su init_resource respete el Fixed.
fn add_physics(app: &mut App) {
    app.insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 1 });
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule());
}

// Escritura de timeline: sin ventana, sin GPU, sin render. El loop corre a tope y
// ManualDuration avanza el reloj exactamente 1/60 por update → cada update = 1 step
// de física = 1 frame de la timeline (mismo timing determinista que --record).
// Los init_asset cubren lo que spawn_object toca (meshes/materiales/escenas); sin
// render nadie los consume, pero los Assets<T> deben existir para que el setup corra.
fn add_headless_plugins(app: &mut App) {
    app.add_plugins(MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::ZERO)))
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(TransformPlugin)
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<bevy::sprite::ColorMaterial>()
        .init_asset::<bevy::text::Font>()
        .init_asset::<Image>()
        .init_asset::<bevy::scene::Scene>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
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
