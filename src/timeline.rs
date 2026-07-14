//! El contrato de datos entre simulate y play — el equivalente de `render_data.rs`
//! en fframes-templates: TODO lo que cruza de la simulación a la actuación pasa
//! por aquí. La maquinaria que lo escribe vive en `plugins/simulate.rs`
//! (SimulatePlugin) y la que lo actúa en `plugins/play.rs` (PlayPlugin).

use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy_rapier3d::prelude::PhysicsSet;
use serde::{Deserialize, Serialize};

/// El vocabulario del juego en la pista de eventos: cómo un evento se escribe
/// como línea de la partitura (payload) y cómo se lee de vuelta (parse). La
/// ADUANA — el enum con su formato, escribir y leer juntos — vive en cada
/// juego; la banda que la transporta es estructura y vive aquí.
pub trait TimelineVocabulary: Event {
    fn payload(&self) -> String;
    fn parse(line: &str) -> Option<Self>
    where
        Self: Sized;
}

/// La etiqueta de la banda: los sistemas del juego que emiten eventos en el
/// mismo tick (sensores, directores) se ordenan `.before(EventBand)` sin
/// conocer a los músicos internos.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventBand;

/// La banda de eventos — igual en todos los juegos y en ambos mundos: la
/// actuación re-emite (sobres → eventos), la física escribe (eventos → buzón)
/// y la escenografía del juego monta, encadenadas en el mismo tick. emit y
/// send son plomería del contrato — solo usan parse/payload; el único músico
/// que pone el juego es su escenografía.
pub fn run_the_event_band<E: TimelineVocabulary, M>(
    app: &mut App,
    stage: impl IntoScheduleConfigs<ScheduleSystem, M>,
) {
    app.add_event::<E>();
    app.add_event::<PlayEvent>();
    app.add_systems(
        FixedUpdate,
        (emit_events_from_timeline::<E>, send_events_to_timeline::<E>, stage)
            .chain()
            .in_set(EventBand)
            .after(PhysicsSet::Writeback),
    );
}

fn emit_events_from_timeline<E: TimelineVocabulary>(
    mut wire: EventReader<PlayEvent>,
    mut events: EventWriter<E>,
) {
    for PlayEvent(payload) in wire.read() {
        events.write(E::parse(payload).unwrap_or_else(|| {
            panic!(
                "evento ilegible en la partitura: '{payload}' — write-timeline y play hablan idiomas distintos"
            )
        }));
    }
}

fn send_events_to_timeline<E: TimelineVocabulary>(
    mut events: EventReader<E>,
    mut timeline: Option<ResMut<TimelineEvents>>,
) {
    let Some(timeline) = timeline.as_deref_mut() else { return };
    for event in events.read() {
        timeline.0.push(event.payload());
    }
}

/// Pose de un cuerpo en un frame: posición + rotación (quaternion xyzw) + escala.
/// La escala viaja porque hay gameplay que la muta (ej. el shrink de canicas);
/// sin ella play renderiza el cuerpo a tamaño completo.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub scale: [f32; 3],
}

/// La partitura: la pose de cada cuerpo en cada frame, a 60 fps.
/// Los cuerpos se identifican por [`TimelineKey`] — simulate y play deben asignar
/// las mismas keys (mismo setup, distinto flag del mismo binario).
/// Se serializa con bincode a `outputs/simulation_<N>s.timeline`.
#[derive(Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    /// `frames[f]` = (TimelineKey, Pose) de cada cuerpo en el frame `f`, orden por key.
    pub frames: Vec<Vec<(u64, Pose)>>,
    /// Eventos opacos del juego por frame, en orden. El engine no interpreta el
    /// payload: el juego los empuja en simulate (TimelineEvents) y los recibe en
    /// play (PlayEvent) para reproducir lo que las poses no capturan — visuales
    /// y despawns disparados por colisión.
    pub events: Vec<(u32, String)>,
}

/// Buzón hacia la partitura: el juego empuja aquí sus eventos durante el
/// FixedUpdate y el engine los asocia al frame en curso. Solo existe en modo
/// simulate — el juego lo toma como `Option<ResMut<TimelineEvents>>` y en los
/// demás modos no paga nada.
#[derive(Resource, Default)]
pub struct TimelineEvents(pub Vec<String>);

/// Un evento de la partitura, re-emitido durante play en su frame original.
#[derive(Event)]
pub struct PlayEvent(pub String);

/// Los dados de la simulación: la única fuente de azar del mundo vivo.
/// Solo están en la mesa donde hay física (nativo y --write-timeline); en
/// --play la suerte ya está echada — quedó escrita en la partitura. Todo
/// sistema del juego que declare `ResMut<Dice>` se duerme solo en play
/// (parámetro ausente → Bevy lo salta): el juego declara lo que necesita,
/// nunca pregunta por modos. La verdad nueva solo nace del choque o del azar,
/// y en play no existe ninguno de los dos.
#[derive(Resource)]
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// SplitMix64 — determinista, pequeño y sin dependencias.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Tira un dado de `n` caras: índice uniforme en 0..n.
    pub fn roll(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Identidad estable de un cuerpo en la timeline, asignada por el juego de forma
/// determinista. Sin ella el mapeo cae al índice de Entity — que Bevy REUTILIZA
/// tras un despawn: si simulate y play despawnean entidades distintas (la
/// utilería visual solo existe en uno de los dos), el orden por Entity diverge y
/// los cuerpos intercambian poses en silencio. Con TimelineKey es por identidad.
#[derive(Component)]
pub struct TimelineKey(pub u64);
