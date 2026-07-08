//! El contrato de datos entre simulate y play — el equivalente de `render_data.rs`
//! en fframes-templates: TODO lo que cruza de la simulación a la actuación pasa
//! por aquí. La maquinaria que lo escribe vive en `plugins/simulate.rs`
//! (SimulatePlugin) y la que lo actúa en `plugins/play.rs` (PlayPlugin).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

/// Identidad estable de un cuerpo en la timeline, asignada por el juego de forma
/// determinista. Sin ella el mapeo cae al índice de Entity — que Bevy REUTILIZA
/// tras un despawn: si simulate y play despawnean entidades distintas (la
/// utilería visual solo existe en uno de los dos), el orden por Entity diverge y
/// los cuerpos intercambian poses en silencio. Con TimelineKey es por identidad.
#[derive(Component)]
pub struct TimelineKey(pub u64);
