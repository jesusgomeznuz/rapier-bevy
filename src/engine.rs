use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::ecs::error::{BevyError, ErrorContext, GLOBAL_ERROR_HANDLER};
use bevy::ecs::system::SystemParamValidationError;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_rapier3d::prelude::*;
use std::time::Duration;

use crate::modes::{record_duration, timeline_path, write_timeline_duration};
use crate::plugins::{PhysicsStatsPlugin, PlayPlugin, RecordPlugin, WriteTimelinePlugin};

pub struct GameAppConfig {
    pub title: &'static str,
    pub resolution: (f32, f32),
    /// El azar de la partida. El juego lo recibe por su puerta (--seed) y lo
    /// entrega aquí; el engine decide si pone dados en la mesa (solo con física).
    pub seed: u64,
}

impl Default for GameAppConfig {
    fn default() -> Self {
        Self {
            title: "rapier-bevy",
            resolution: (1280.0, 720.0),
            seed: 0,
        }
    }
}

pub fn random_physics_game_app(config: GameAppConfig) -> App {
    let writing_timeline = write_timeline_duration();

    // El manejador global se fija ANTES de construir el App: Bevy lo cachea
    // con get_or_init en cuanto arma sus schedules, y después ya nadie lo cambia.
    if writing_timeline.is_none() && timeline_path().is_some() {
        let _ = GLOBAL_ERROR_HANDLER.set(sleep_systems_with_absent_needs);
    }

    let mut app = App::new();
    match (writing_timeline, record_duration()) {
        (Some(_), _)       => add_headless_plugins(&mut app),
        (None, Some(secs)) => { app.add_plugins(RecordPlugin { duration_secs: secs }); }
        (None, None)       => add_windowed_plugins(&mut app, &config),
    }

    // Toda la simulación corre en FixedUpdate a 60 steps/s. Ese es el reloj de verdad
    // del juego; los modificadores --record y --write-timeline aceleran el wall-clock para
    // generar simulaciones largas rápido, sin alterar la duración lógica de nada.
    app.insert_resource(Time::<Fixed>::from_hz(60.0));

    match (writing_timeline, timeline_path()) {
        // Escribir timeline: física + captura; gana sobre --record/--play.
        (Some(secs), _) => {
            add_physics(&mut app, config.seed);
            app.add_plugins(WriteTimelinePlugin { duration_secs: secs });
        }
        // Play: SIN física — la timeline dicta los Transforms y Bevy solo dibuja.
        // Combina con --record (video) o con ventana (preview). Aquí no se pone
        // NI física NI dados en la mesa: los sistemas del juego que declaran
        // choques (EventReader<CollisionEvent>) o azar (ResMut<Dice>) se
        // duermen solos por necesidad ausente. El juego nunca supo de modos.
        (None, Some(path)) => {
            app.add_plugins(PlayPlugin { path });
        }
        (None, None) => {
            add_physics(&mut app, config.seed);
        }
    }

    app
}

// En play un sistema del juego puede declarar necesidades que este mundo no
// tiene (choques, dados). Bevy reporta esa carencia como error de validación
// de parámetros; aquí se convierte en sueño — el sistema se salta el tick —
// porque su verdad ya viene escrita en la partitura. Todo lo demás truena
// igual que siempre: nada falla en silencio salvo el dormir diseñado.
fn sleep_systems_with_absent_needs(error: BevyError, ctx: ErrorContext) {
    if error.downcast_ref::<SystemParamValidationError>().is_some() {
        bevy::log::debug_once!("duerme en play: {} — {}", ctx.name(), error);
        return;
    }
    bevy::ecs::error::panic(error, ctx);
}

// El TimestepMode va antes del plugin para que su init_resource respete el Fixed.
// Donde hay física hay dados: la única fuente de azar del mundo vivo.
fn add_physics(app: &mut App, seed: u64) {
    app.insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 1 });
    app.insert_resource(crate::timeline::Dice::new(seed));
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
