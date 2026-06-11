# rapier-bevy

3D physics engine using [Bevy](https://bevyengine.org/) + [Rapier](https://rapier.rs/). Two roles: portfolio demo with flowchart-style architecture, and library consumed by game binaries as a path dependency (`engine::game_app` provides window/physics/recording for free).

## Modes

```bash
cargo run -- --preprocess          # VHACD offline → .compound (bincode) + .compound.json
cargo run                          # sim with precomputed colliders (fast)
cargo run -- --sim-raw             # VHACD at runtime (slow, for comparison)
cargo run -- --debug               # show collider wireframes
cargo run -- --bench falling-spheres 200   # benchmark: avg FPS + p01 over 600 frames
```

### Recording (headless GPU → ffmpeg)

```bash
cargo run --release -- --record 60   # 60s of video → outputs/record_60s.mp4
```

No window: offscreen render → GPU compute RGBA→yuv420p → async readback → parallel libx264 encoders → concat. Runs at ~6–9x realtime on an M4 Pro. Flags and performance history in [`docs/record-perf.md`](docs/record-perf.md).

### Bake / replay (physics decoupled from rendering)

```bash
cargo run --release -- --bake 60                                       # physics only, no GPU (~65x) → outputs/bake_60s.timeline
cargo run --release -- --record 60 --replay outputs/bake_60s.timeline  # render video from a baked timeline (no physics)
cargo run -- --replay outputs/bake_60s.timeline                        # live window preview of a timeline
```

VFX "point cache" pattern: `--bake` runs the simulation headless and stores every body's pose per frame; `--replay` plays it back by setting `Transform`s directly, no Rapier involved. Run the cheap part (physics) many times to find the result you want, render the expensive part (video) once. Constraint: bake and replay must spawn the same world in the same order (same setup, different flag of the same binary).

## Expected output

```
[preprocess]      total: ~2s
[sim-precomputed] setup_world: ~3ms
[sim-raw]         setup_world: ~1.5s
[bake]            600 frames (10s de sim) en 0.15s → 65x realtime
```

## Stack

- Rust · Bevy 0.16 · bevy_rapier3d 0.31
