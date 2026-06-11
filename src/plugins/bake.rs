use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

const FPS: u32 = 60;

/// Pose de un cuerpo en un frame: posición + rotación (quaternion xyzw).
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Pose {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

/// Simulación horneada: la pose de cada cuerpo en cada frame, a 60 fps.
/// `frames[f][b]` = pose del cuerpo `b` en el frame `f`. El orden de los cuerpos
/// es por Entity ascendente — estable mientras bake y replay spawneen el mismo
/// mundo en el mismo orden (mismo setup, distinto flag del mismo binario).
#[derive(Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    pub frames: Vec<Vec<Pose>>,
}

// ── Bake: correr solo la física y grabar la timeline ─────────────────────

pub struct BakePlugin {
    pub duration_secs: u32,
}

#[derive(Resource)]
struct BakeState {
    total_frames: u32,
    frames: Vec<Vec<Pose>>,
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
            output: PathBuf::from("outputs").join(format!("bake_{}s.timeline", self.duration_secs)),
            t_start: Instant::now(),
        })
        // Tras el writeback los Transform ya tienen el resultado del step: una
        // captura por step = un frame de video futuro (mismo 1:1 que --record).
        .add_systems(FixedUpdate, capture_frame.after(PhysicsSet::Writeback))
        .add_systems(Update, check_bake_complete);
    }
}

fn capture_frame(
    mut state: ResMut<BakeState>,
    bodies: Query<(Entity, &Transform), With<RigidBody>>,
) {
    if state.frames.len() as u32 >= state.total_frames {
        return;
    }

    let mut rows: Vec<(Entity, Pose)> = bodies
        .iter()
        .map(|(entity, t)| {
            (entity, Pose { pos: t.translation.to_array(), rot: t.rotation.to_array() })
        })
        .collect();
    rows.sort_by_key(|(entity, _)| *entity);

    state.frames.push(rows.into_iter().map(|(_, pose)| pose).collect());
}

fn check_bake_complete(mut state: ResMut<BakeState>, mut exit: EventWriter<AppExit>) {
    if (state.frames.len() as u32) < state.total_frames {
        return;
    }

    let timeline = Timeline { fps: FPS, frames: std::mem::take(&mut state.frames) };
    let data = bincode::serialize(&timeline).expect("[bake] failed to serialize timeline");
    std::fs::write(&state.output, &data)
        .unwrap_or_else(|_| panic!("[bake] failed to write {}", state.output.display()));

    let secs = state.t_start.elapsed().as_secs_f64();
    let sim_secs = state.total_frames as f64 / FPS as f64;
    let bodies = timeline.frames.first().map(Vec::len).unwrap_or(0);
    println!(
        "[bake] {} frames ({}s de sim) en {:.2}s → {:.0}x realtime",
        state.total_frames, sim_secs, secs, sim_secs / secs,
    );
    println!(
        "[bake] {} ready ({:.1} MB, {} cuerpos)",
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
}

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        let data = std::fs::read(&self.path)
            .unwrap_or_else(|_| panic!("[replay] cannot read {}", self.path.display()));
        let timeline: Timeline =
            bincode::deserialize(&data).expect("[replay] failed to deserialize timeline");

        println!(
            "[replay] {} — {} frames ({}s), {} cuerpos",
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
        app.insert_resource(ReplayState { timeline, cursor: 0 })
            .add_systems(FixedUpdate, apply_replay_frame.in_set(PhysicsSet::Writeback));
    }
}

fn apply_replay_frame(
    mut state: ResMut<ReplayState>,
    mut bodies: Query<(Entity, &mut Transform), With<RigidBody>>,
) {
    let cursor = state.cursor;
    if cursor >= state.timeline.frames.len() {
        return; // timeline agotada: el mundo se congela en la última pose
    }
    state.cursor += 1;
    let frame = &state.timeline.frames[cursor];

    let mut rows: Vec<(Entity, Mut<Transform>)> = bodies.iter_mut().collect();
    assert_eq!(
        rows.len(),
        frame.len(),
        "[replay] el mundo tiene {} cuerpos pero la timeline {} — bake y replay deben spawnear el mismo mundo",
        rows.len(),
        frame.len(),
    );
    rows.sort_by_key(|(entity, _)| *entity);

    for ((_, transform), pose) in rows.iter_mut().zip(frame) {
        transform.translation = Vec3::from_array(pose.pos);
        transform.rotation = Quat::from_array(pose.rot);
    }
}
