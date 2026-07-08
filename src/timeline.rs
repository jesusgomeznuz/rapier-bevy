//! El contrato de datos entre bake y replay — el equivalente de `render_data.rs`
//! en fframes-templates: TODO lo que cruza de la simulación a la actuación pasa
//! por aquí. La maquinaria que lo escribe y lo lee vive en `plugins/bake.rs`
//! (BakePlugin / ReplayPlugin).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Pose de un cuerpo en un frame: posición + rotación (quaternion xyzw) + escala.
/// La escala viaja porque hay gameplay que la muta (ej. el shrink de canicas);
/// sin ella el replay renderiza el cuerpo a tamaño completo.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub scale: [f32; 3],
}

/// Simulación horneada: la pose de cada cuerpo en cada frame, a 60 fps.
/// Los cuerpos se identifican por [`BakeKey`] — bake y replay deben asignar las
/// mismas keys (mismo setup, distinto flag del mismo binario).
/// Se serializa con bincode a `outputs/bake_<N>s.timeline`.
#[derive(Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    /// `frames[f]` = (BakeKey, Pose) de cada cuerpo en el frame `f`, orden por key.
    pub frames: Vec<Vec<(u64, Pose)>>,
    /// Eventos opacos del juego por frame, en orden. El engine no interpreta el
    /// payload: el juego los empuja en bake (BakeEvents) y los recibe en replay
    /// (ReplayEvent) para reproducir lo que las poses no capturan — visuales y
    /// despawns disparados por colisión.
    pub events: Vec<(u32, String)>,
}

/// Buzón del bake: el juego empuja aquí sus eventos durante el FixedUpdate y el
/// engine los asocia al frame en curso. Solo existe en modo bake — el juego lo
/// toma como `Option<ResMut<BakeEvents>>` y en los demás modos no paga nada.
#[derive(Resource, Default)]
pub struct BakeEvents(pub Vec<String>);

/// Un evento horneado, re-emitido durante el replay en su frame original.
#[derive(Event)]
pub struct ReplayEvent(pub String);

/// Identidad estable de un cuerpo en la timeline, asignada por el juego de forma
/// determinista. Sin ella el mapeo cae al índice de Entity — que Bevy REUTILIZA
/// tras un despawn: si bake y replay despawnean entidades distintas (la utilería
/// visual solo existe en uno de los dos), el orden por Entity diverge y los
/// cuerpos intercambian poses en silencio. Con BakeKey el mapeo es por identidad.
#[derive(Component)]
pub struct BakeKey(pub u64);
