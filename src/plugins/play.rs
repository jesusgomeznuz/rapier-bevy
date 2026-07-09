use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::path::PathBuf;

use crate::timeline::{PlayEvent, Timeline, TimelineKey};

pub struct PlayPlugin {
    pub path: PathBuf,
}

#[derive(Resource)]
struct PlayState {
    timeline: Timeline,
    cursor: usize,
    next_event: usize,
}

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let data = std::fs::read(&self.path)
            .unwrap_or_else(|_| panic!("[play] cannot read {}", self.path.display()));
        let timeline: Timeline =
            bincode::deserialize(&data).expect("[play] failed to deserialize timeline");

        println!(
            "[play] {} — {} frames ({}s), {} cuerpos",
            self.path.display(),
            timeline.frames.len(),
            timeline.frames.len() as u32 / timeline.fps.max(1),
            timeline.frames.first().map(Vec::len).unwrap_or(0),
        );

        // La actuación ES el writeback: ocupa PhysicsSet::Writeback para que los
        // sistemas del juego ordenados con .after(Writeback) vean las poses de ESTE
        // frame, igual que con física real. Sin esto corren con poses del frame
        // anterior y el mundo diverge de la simulación (spawns desfasados →
        // mismatch de cuerpos contra la timeline).
        app.insert_resource(PlayState { timeline, cursor: 0, next_event: 0 })
            .add_event::<PlayEvent>()
            .add_systems(FixedUpdate, apply_timeline_frame.in_set(PhysicsSet::Writeback));
    }
}

// La partitura mueve lo que lleva TimelineKey; el filtro RigidBody queda como
// respaldo para cuerpos físicos sin key (juegos donde todo lo posado ES física).
// Los juegos sin física (musical-path) posan actores puros: Transform + key.
fn apply_timeline_frame(
    mut state: ResMut<PlayState>,
    mut bodies: Query<
        (Entity, Option<&TimelineKey>, &mut Transform),
        Or<(With<RigidBody>, With<TimelineKey>)>,
    >,
    mut events: EventWriter<PlayEvent>,
) {
    let cursor = state.cursor;
    if cursor >= state.timeline.frames.len() {
        return; // timeline agotada: el mundo se congela en la última pose
    }
    state.cursor += 1;

    // Re-emite los eventos de la partitura de este frame; los sistemas del juego
    // ordenados .after(Writeback) los leen en el mismo tick.
    while let Some((frame, payload)) = state.timeline.events.get(state.next_event) {
        if *frame as usize > cursor {
            break; // ordenados por frame: lo que sigue es del futuro
        }
        events.write(PlayEvent(payload.clone()));
        state.next_event += 1;
    }
    let frame = &state.timeline.frames[cursor];

    let mut rows: Vec<(u64, Mut<Transform>)> = bodies
        .iter_mut()
        .map(|(entity, key, transform)| {
            (key.map(|k| k.0).unwrap_or(u64::from(entity.index())), transform)
        })
        .collect();
    assert_eq!(
        rows.len(),
        frame.len(),
        "[play] el mundo tiene {} cuerpos pero la timeline {} — simulate y play deben spawnear el mismo mundo",
        rows.len(),
        frame.len(),
    );
    rows.sort_by_key(|(key, _)| *key);

    for ((world_key, transform), (timeline_key, pose)) in rows.iter_mut().zip(frame) {
        assert_eq!(
            world_key, timeline_key,
            "[play] el mundo tiene el cuerpo {world_key} donde la timeline trae {timeline_key} — \
             simulate y play asignaron TimelineKeys distintas",
        );
        transform.translation = Vec3::from_array(pose.pos);
        transform.rotation = Quat::from_array(pose.rot);
        transform.scale = Vec3::from_array(pose.scale);
    }
}
