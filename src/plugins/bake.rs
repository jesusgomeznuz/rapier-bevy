use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

use crate::timeline::{BakeEvents, BakeKey, Pose, ReplayEvent, Timeline};

const FPS: u32 = 60;

// ── Bake: correr solo la física y grabar la timeline ─────────────────────

pub struct BakePlugin {
    pub duration_secs: u32,
}

#[derive(Resource)]
struct BakeState {
    total_frames: u32,
    frames: Vec<Vec<(u64, Pose)>>,
    events: Vec<(u32, String)>,
    output: PathBuf,
    t_start: Instant,
}

impl Plugin for BakePlugin {
    fn build(&self, app: &mut App) {
        let total_frames = FPS * self.duration_secs;
        std::fs::create_dir_all("outputs").expect("cannot create outputs/");

        app.insert_resource(BakeState {
            total_frames,
            frames: Vec::with_capacity(total_frames as usize),
            events: Vec::new(),
            output: PathBuf::from("outputs").join(format!("simulation_{}s.timeline", self.duration_secs)),
            t_start: Instant::now(),
        })
        .init_resource::<BakeEvents>()
        // Tras el writeback los Transform ya tienen el resultado del step: una
        // captura por step = un frame de video futuro (mismo 1:1 que --record).
        .add_systems(FixedUpdate, capture_frame.after(PhysicsSet::Writeback))
        // En FixedPostUpdate todos los sistemas del juego del tick ya empujaron:
        // lo pendiente pertenece al frame recién capturado.
        .add_systems(bevy::app::FixedPostUpdate, drain_frame_events)
        .add_systems(Update, check_bake_complete);
    }
}

fn capture_frame(
    mut state: ResMut<BakeState>,
    bodies: Query<(Entity, Option<&BakeKey>, &Transform), With<RigidBody>>,
) {
    if state.frames.len() as u32 >= state.total_frames {
        return;
    }

    let mut rows: Vec<(u64, Pose)> = bodies
        .iter()
        .map(|(entity, key, t)| {
            (
                // Sin BakeKey cae al índice de Entity — válido solo en mundos sin
                // despawns (la demo); con despawns el juego DEBE asignar keys.
                key.map(|k| k.0).unwrap_or(u64::from(entity.index())),
                Pose {
                    pos: t.translation.to_array(),
                    rot: t.rotation.to_array(),
                    scale: t.scale.to_array(),
                },
            )
        })
        .collect();
    rows.sort_by_key(|(key, _)| *key);
    for pair in rows.windows(2) {
        assert_ne!(
            pair[0].0, pair[1].0,
            "[simulate] BakeKey duplicada ({}) — el mapeo de poses sería ambiguo",
            pair[0].0,
        );
    }

    state.frames.push(rows);
}

fn drain_frame_events(mut pending: ResMut<BakeEvents>, mut state: ResMut<BakeState>) {
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

fn check_bake_complete(mut state: ResMut<BakeState>, mut exit: EventWriter<AppExit>) {
    if (state.frames.len() as u32) < state.total_frames {
        return;
    }

    let timeline = Timeline {
        fps: FPS,
        frames: std::mem::take(&mut state.frames),
        events: std::mem::take(&mut state.events),
    };
    let data = bincode::serialize(&timeline).expect("[simulate] failed to serialize timeline");
    std::fs::write(&state.output, &data)
        .unwrap_or_else(|_| panic!("[simulate] failed to write {}", state.output.display()));

    let secs = state.t_start.elapsed().as_secs_f64();
    let sim_secs = state.total_frames as f64 / FPS as f64;
    let bodies = timeline.frames.first().map(Vec::len).unwrap_or(0);
    println!(
        "[simulate] {} frames ({}s de sim) en {:.2}s → {:.0}x realtime",
        state.total_frames, sim_secs, secs, sim_secs / secs,
    );
    println!(
        "[simulate] {} ready ({:.1} MB, {} cuerpos)",
        state.output.display(), data.len() as f64 / 1e6, bodies,
    );

    exit.write(AppExit::Success);
}

// ── Replay: setear Transforms desde la timeline, sin física ──────────────

pub struct ReplayPlugin {
    pub path: PathBuf,
}

#[derive(Resource)]
struct ReplayState {
    timeline: Timeline,
    cursor: usize,
    next_event: usize,
}

impl Plugin for ReplayPlugin {
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

        // El replay ES el writeback: ocupa PhysicsSet::Writeback para que los sistemas
        // del juego ordenados con .after(Writeback) — p.ej. generación progresiva de
        // nivel — vean las poses de ESTE frame, igual que con física real. Sin esto
        // corren con poses del frame anterior y el mundo diverge del bake (spawns
        // desfasados → mismatch de cuerpos contra la timeline).
        app.insert_resource(ReplayState { timeline, cursor: 0, next_event: 0 })
            .add_event::<ReplayEvent>()
            .add_systems(FixedUpdate, apply_replay_frame.in_set(PhysicsSet::Writeback));
    }
}

fn apply_replay_frame(
    mut state: ResMut<ReplayState>,
    mut bodies: Query<(Entity, Option<&BakeKey>, &mut Transform), With<RigidBody>>,
    mut events: EventWriter<ReplayEvent>,
) {
    let cursor = state.cursor;
    if cursor >= state.timeline.frames.len() {
        return; // timeline agotada: el mundo se congela en la última pose
    }
    state.cursor += 1;

    // Re-emite los eventos horneados de este frame; los sistemas del juego
    // ordenados .after(Writeback) los leen en el mismo tick.
    while let Some((frame, payload)) = state.timeline.events.get(state.next_event) {
        if *frame as usize > cursor {
            break; // ordenados por frame: lo que sigue es del futuro
        }
        events.write(ReplayEvent(payload.clone()));
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
        "[play] el mundo tiene {} cuerpos pero la timeline {} — bake y replay deben spawnear el mismo mundo",
        rows.len(),
        frame.len(),
    );
    rows.sort_by_key(|(key, _)| *key);

    for ((world_key, transform), (baked_key, pose)) in rows.iter_mut().zip(frame) {
        assert_eq!(
            world_key, baked_key,
            "[play] el mundo tiene el cuerpo {world_key} donde la timeline trae {baked_key} — \
             bake y replay asignaron BakeKeys distintas",
        );
        transform.translation = Vec3::from_array(pose.pos);
        transform.rotation = Quat::from_array(pose.rot);
        transform.scale = Vec3::from_array(pose.scale);
    }
}
