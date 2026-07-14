use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

use crate::timeline::{Pose, Timeline, TimelineEvents, TimelineKey};

const FPS: u32 = 60;

pub struct WriteTimelinePlugin {
    pub duration_secs: u32,
}

#[derive(Resource)]
struct WriteTimelineState {
    total_frames: u32,
    frames: Vec<Vec<(u64, Pose)>>,
    events: Vec<(u32, String)>,
    output: PathBuf,
    t_start: Instant,
}

impl Plugin for WriteTimelinePlugin {
    fn build(&self, app: &mut App) {
        let total_frames = FPS * self.duration_secs;
        std::fs::create_dir_all("outputs").expect("cannot create outputs/");

        app.insert_resource(WriteTimelineState {
            total_frames,
            frames: Vec::with_capacity(total_frames as usize),
            events: Vec::new(),
            output: PathBuf::from("outputs")
                .join(format!("simulation_{}s.timeline", self.duration_secs)),
            t_start: Instant::now(),
        })
        .init_resource::<TimelineEvents>()
        // Tras el writeback los Transform ya tienen el resultado del step: una
        // captura por step = un frame de video futuro (mismo 1:1 que --record).
        .add_systems(FixedUpdate, capture_frame.after(PhysicsSet::Writeback))
        // En FixedPostUpdate todos los sistemas del juego del tick ya empujaron:
        // lo pendiente pertenece al frame recién capturado.
        .add_systems(bevy::app::FixedPostUpdate, drain_frame_events)
        .add_systems(Update, check_timeline_complete);
    }
}

fn capture_frame(
    mut state: ResMut<WriteTimelineState>,
    // Mismo filtro que `play::apply_timeline_frame` — simulate y play deben
    // spawnear el mismo mundo. Los objetos de nivel (world/modules.rs) llevan
    // TimelineKey pero, si son BodyType::Static, no llevan RigidBody; sin el
    // Or aquí, write-timeline los ignoraba y play sí los contaba (mismatch).
    bodies: Query<(Entity, Option<&TimelineKey>, &Transform), Or<(With<RigidBody>, With<TimelineKey>)>>,
) {
    if state.frames.len() as u32 >= state.total_frames {
        return;
    }

    let mut rows: Vec<(u64, Pose)> = bodies
        .iter()
        .map(|(entity, key, transform)| {
            (
                // Sin TimelineKey cae al índice de Entity — válido solo en mundos sin
                // despawns (la demo); con despawns el juego DEBE asignar keys.
                key.map(|k| k.0).unwrap_or(u64::from(entity.index())),
                Pose {
                    pos: transform.translation.to_array(),
                    rot: transform.rotation.to_array(),
                    scale: transform.scale.to_array(),
                },
            )
        })
        .collect();
    rows.sort_by_key(|(key, _)| *key);
    for pair in rows.windows(2) {
        assert_ne!(
            pair[0].0, pair[1].0,
            "[write-timeline] TimelineKey duplicada ({}) — el mapeo de poses sería ambiguo",
            pair[0].0,
        );
    }

    state.frames.push(rows);
}

fn drain_frame_events(mut pending: ResMut<TimelineEvents>, mut state: ResMut<WriteTimelineState>) {
    if pending.0.is_empty() {
        return;
    }
    let Some(frame) = state.frames.len().checked_sub(1) else {
        pending.0.clear(); // eventos antes del primer frame capturado: no hay dónde anclarlos
        return;
    };
    for payload in pending.0.drain(..) {
        state.events.push((frame as u32, payload));
    }
}

fn check_timeline_complete(mut state: ResMut<WriteTimelineState>, mut exit: EventWriter<AppExit>) {
    if (state.frames.len() as u32) < state.total_frames {
        return;
    }

    let timeline = Timeline {
        fps: FPS,
        frames: std::mem::take(&mut state.frames),
        events: std::mem::take(&mut state.events),
    };
    let data = bincode::serialize(&timeline).expect("[write-timeline] failed to serialize timeline");
    std::fs::write(&state.output, &data)
        .unwrap_or_else(|_| panic!("[write-timeline] failed to write {}", state.output.display()));

    let secs = state.t_start.elapsed().as_secs_f64();
    let sim_secs = state.total_frames as f64 / FPS as f64;
    let bodies = timeline.frames.first().map(Vec::len).unwrap_or(0);
    println!(
        "[write-timeline] {} frames ({}s de sim) en {:.2}s → {:.0}x realtime",
        state.total_frames, sim_secs, secs, sim_secs / secs,
    );
    println!(
        "[write-timeline] {} ready ({:.1} MB, {} cuerpos)",
        state.output.display(),
        data.len() as f64 / 1e6,
        bodies,
    );

    exit.write(AppExit::Success);
}
