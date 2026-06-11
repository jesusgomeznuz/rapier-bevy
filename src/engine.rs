use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::render::RapierDebugRenderPlugin;
use std::time::Duration;

use crate::modes::{SimMode, bake_duration, debug_enabled, record_duration, replay_path};
use crate::plugins::{BakePlugin, PhysicsStatsPlugin, RecordPlugin, ReplayPlugin};

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

    let baking = bake_duration();
    match (baking, record_duration()) {
        (Some(_), _)       => add_headless_plugins(&mut app),
        (None, Some(secs)) => { app.add_plugins(RecordPlugin { duration_secs: secs }); }
        (None, None)       => add_windowed_plugins(&mut app, &config),
    }

    // Toda la simulación corre en FixedUpdate a 60 steps/s. Ese es el reloj de verdad
    // del juego; los modificadores --record y --bake aceleran el wall-clock para
    // generar simulaciones largas rápido, sin alterar la duración lógica de nada.
    app.insert_resource(Time::<Fixed>::from_hz(60.0));

    match (baking, replay_path()) {
        // Bake: física + captura de timeline; gana sobre --record/--replay.
        (Some(secs), _) => {
            add_physics(&mut app);
            app.add_plugins(BakePlugin { duration_secs: secs });
        }
        // Replay: SIN física — la timeline horneada dicta los Transforms y Bevy
        // solo dibuja. Combina con --record (video) o con ventana (preview).
        (None, Some(path)) => {
            app.add_plugins(ReplayPlugin { path });
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

// El TimestepMode va antes del plugin para que su init_resource respete el Fixed.
fn add_physics(app: &mut App) {
    app.insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 1 });
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule());
}

// Bake: sin ventana, sin GPU, sin render. El loop corre tan rápido como puede y
// ManualDuration avanza el reloj exactamente 1/60 por update → cada update = 1 step
// de física = 1 frame de la timeline (mismo timing determinista que --record).
// Los init_asset cubren lo que spawn_object toca (meshes/materiales/escenas); sin
// render nadie los consume, pero los Assets<T> deben existir para que el setup corra.
fn add_headless_plugins(app: &mut App) {
    app.add_plugins(MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::ZERO)))
        .add_plugins(TransformPlugin)
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<bevy::sprite::ColorMaterial>()
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
