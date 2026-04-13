# rapier-bevy

3D physics benchmark using [Bevy](https://bevyengine.org/) + [Rapier](https://rapier.rs/).

Demonstrates the performance difference between loading preprocessed complex colliders vs. computing them at runtime with VHACD.

## Usage

### 1. Preprocess assets

Generates `.compound` files from `.obj` files in `assets/`:

```bash
cargo run -- --preprocess
```

### 2. Simulation with preprocessed colliders *(fast)*

```bash
cargo run
```

### 3. Simulation with VHACD at runtime *(slow, for comparison)*

```bash
cargo run -- --sim-raw
```

## Expected output

```
[preprocess]      total: ~2s
[sim-precomputed] setup_world: ~3ms
[sim-raw]         setup_world: ~1.5s
```

## Stack

- Rust · Bevy 0.16 · bevy_rapier3d 0.31
