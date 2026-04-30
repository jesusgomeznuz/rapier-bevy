use bevy::{
    prelude::*,
    render::{
        Extract, Render, RenderApp, RenderSet,
        camera::RenderTarget,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel},
        render_resource::{
            Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d,
            Maintain, MapMode, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureDimension,
            TextureFormat, TextureUsages,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
    winit::WinitPlugin,
};
use crossbeam_channel::{Receiver, Sender};
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Duration,
};

const WIDTH:    u32 = 1080;
const HEIGHT:   u32 = 1920;
const FPS:      u32 = 60;
const SPEED:    f32 = 50.0; // virtual time multiplier — sweet spot on M4

/// Inserted by RecordPlugin before GraphicsPlugin runs,
/// so the camera renders to this image instead of the window.
#[derive(Resource)]
pub struct OffscreenTarget {
    pub image: Handle<Image>,
}

/// Handles de assets que deben estar cargados antes de empezar a grabar.
/// Poblar desde el juego con los handles de imágenes cargadas.
#[derive(Resource, Default)]
pub struct AssetsLoading(pub Vec<UntypedHandle>);

#[derive(Resource)]
struct RecordState {
    frames_captured: u32,
    total_frames:    u32,
    frames_to_skip:  u32,
    capture_pending: bool,
    finalized:       bool,
    output_path:     PathBuf,
    ffmpeg_child:    Option<Child>,
    ffmpeg_stdin:    Option<ChildStdin>,
    row_bytes:       usize,
    padded_bytes:    usize,
}

#[derive(Resource)]
struct MainWorldReceiver(Receiver<Vec<u8>>);

#[derive(Resource)]
struct RenderWorldSender(Sender<Vec<u8>>);

pub struct RecordPlugin {
    pub duration_secs: u32,
}

impl Plugin for RecordPlugin {
    fn build(&self, app: &mut App) {
        let total_frames = FPS * self.duration_secs;
        let output_path  = PathBuf::from("outputs")
            .join(format!("record_{}s.mp4", self.duration_secs));
        std::fs::create_dir_all("outputs").expect("cannot create outputs/");

        let row_bytes   = WIDTH as usize * 4;
        let padded_bytes = {
            let align = 256usize;
            row_bytes + (align - row_bytes % align) % align
        };

        let pix_fmt     = "rgba"; // offscreen Image usa TextureFormat::bevy_default() = RGBA en todas las plataformas
        let video_codec = if cfg!(target_os = "macos") { "h264_videotoolbox" } else { "libx264" };

        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",         "rawvideo",
                "-pix_fmt",   pix_fmt,
                "-s:v",       &format!("{}x{}", WIDTH, HEIGHT),
                "-framerate", &FPS.to_string(),
                "-i",         "pipe:0",
                "-c:v",       video_codec,
                "-pix_fmt",   "yuv420p",
                output_path.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .expect("[record] ffmpeg not found — install with: brew install ffmpeg");

        let ffmpeg_stdin = ffmpeg.stdin.take();

        let (sender, receiver) = crossbeam_channel::unbounded::<Vec<u8>>();

        app.add_plugins(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: None,
                        exit_condition:  bevy::window::ExitCondition::DontExit,
                        ..default()
                    })
                    .disable::<WinitPlugin>(),
            )
            .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::ZERO));

        // Advance virtual time faster than wall time → physics runs at SPEED×
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_relative_speed(SPEED);

        app
            .insert_resource(Time::<Fixed>::from_hz(FPS as f64))
            .insert_resource(MainWorldReceiver(receiver))
            .insert_resource(AssetsLoading::default())
            .insert_resource(RecordState {
                frames_captured: 0,
                total_frames,
                frames_to_skip:  3,
                capture_pending: false,
                finalized: false,
                output_path,
                ffmpeg_child: Some(ffmpeg),
                ffmpeg_stdin,
                row_bytes,
                padded_bytes,
            })
            .add_systems(PreStartup, create_offscreen_target)
            .add_systems(FixedUpdate, mark_capture)
            .add_systems(Update, (receive_and_pipe, check_complete).chain());

        let render_app = app.sub_app_mut(RenderApp);
        let mut graph  = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ImageCopyLabel, ImageCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopyLabel);

        render_app
            .insert_resource(RenderWorldSender(sender))
            .add_systems(ExtractSchedule, image_copy_extract)
            .add_systems(Render, copy_buffer_to_channel.after(RenderSet::Render));
    }
}

fn create_offscreen_target(
    mut commands:  Commands,
    mut images:    ResMut<Assets<Image>>,
    render_device: Res<RenderDevice>,
) {
    let size = Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 };

    let mut render_target = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0; 4],
        TextureFormat::bevy_default(),
        RenderAssetUsages::default(),
    );
    render_target.texture_descriptor.usage |=
        TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;

    let handle = images.add(render_target);

    commands.insert_resource(OffscreenTarget { image: handle.clone() });
    commands.spawn(ImageCopier::new(handle, size, &render_device));
}

fn mark_capture(
    mut state: ResMut<RecordState>,
    loading: Res<AssetsLoading>,
    asset_server: Res<AssetServer>,
) {
    let all_ready = loading.0.iter().all(|h| asset_server.is_loaded_with_dependencies(h));
    if !all_ready {
        return;
    }
    if state.frames_captured < state.total_frames {
        state.capture_pending = true;
    }
}

fn receive_and_pipe(receiver: Res<MainWorldReceiver>, mut state: ResMut<RecordState>) {
    if state.finalized {
        return;
    }

    if !state.capture_pending {
        while receiver.0.try_recv().is_ok() {} // drain to avoid backlog
        return;
    }

    // Drain channel — keep only the last (most recent physics state)
    let mut last: Option<Vec<u8>> = None;
    while let Ok(data) = receiver.0.try_recv() {
        last = Some(data);
    }

    let Some(raw) = last else { return };

    if state.frames_to_skip > 0 {
        state.frames_to_skip -= 1;
        state.capture_pending = false;
        return;
    }

    // Strip wgpu row padding before piping
    let row_bytes    = state.row_bytes;
    let padded_bytes = state.padded_bytes;
    if let Some(ref mut stdin) = state.ffmpeg_stdin {
        if row_bytes == padded_bytes {
            let _ = stdin.write_all(&raw);
        } else {
            for row in raw.chunks(padded_bytes) {
                let _ = stdin.write_all(&row[..row_bytes.min(row.len())]);
            }
        }
    }

    state.capture_pending   = false;
    state.frames_captured  += 1;

    if state.frames_captured % 60 == 0 {
        let pct = state.frames_captured * 100 / state.total_frames;
        println!("[record] {}/{} frames ({}%)", state.frames_captured, state.total_frames, pct);
    }
}

fn check_complete(mut state: ResMut<RecordState>, mut exit: EventWriter<AppExit>) {
    if state.finalized || state.frames_captured < state.total_frames {
        return;
    }
    state.finalized = true;

    drop(state.ffmpeg_stdin.take());
    println!("[record] {} frames done, waiting for ffmpeg…", state.frames_captured);

    if let Some(mut child) = state.ffmpeg_child.take() {
        match child.wait() {
            Ok(s) if s.success() => println!("[record] {} ready", state.output_path.display()),
            Ok(s)                => eprintln!("[record] ffmpeg exited with {s}"),
            Err(e)               => eprintln!("[record] ffmpeg error: {e}"),
        }
    }

    exit.write(AppExit::Success);
}

// ── RenderGraph: GPU texture → CPU buffer → channel ──────────────────────

#[derive(Clone, Component)]
struct ImageCopier {
    buffer:    Buffer,
    src_image: Handle<Image>,
    enabled:   Arc<AtomicBool>,
}

impl ImageCopier {
    fn new(src_image: Handle<Image>, size: Extent3d, device: &RenderDevice) -> Self {
        let padded = RenderDevice::align_copy_bytes_per_row(size.width as usize * 4);
        let buffer = device.create_buffer(&BufferDescriptor {
            label:              Some("record_readback"),
            size:               (padded * size.height as usize) as u64,
            usage:              BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ImageCopier { buffer, src_image, enabled: Arc::new(AtomicBool::new(true)) }
    }
}

#[derive(Clone, Default, Resource, Deref, DerefMut)]
struct ImageCopiers(Vec<ImageCopier>);

fn image_copy_extract(mut commands: Commands, copiers: Extract<Query<&ImageCopier>>) {
    commands.insert_resource(ImageCopiers(copiers.iter().cloned().collect()));
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, RenderLabel)]
struct ImageCopyLabel;

struct ImageCopyDriver;

impl render_graph::Node for ImageCopyDriver {
    fn run(
        &self,
        _graph:         &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world:          &World,
    ) -> Result<(), NodeRunError> {
        let copiers    = world.resource::<ImageCopiers>();
        let gpu_images = world.resource::<RenderAssets<bevy::render::texture::GpuImage>>();

        for copier in copiers.iter() {
            let Some(src) = gpu_images.get(&copier.src_image) else { continue };

            let block_dim  = src.texture_format.block_dimensions();
            let block_size = src.texture_format.block_copy_size(None).unwrap();
            let padded     = RenderDevice::align_copy_bytes_per_row(
                (src.size.width as usize / block_dim.0 as usize) * block_size as usize,
            );

            let mut encoder = render_context
                .render_device()
                .create_command_encoder(&CommandEncoderDescriptor::default());

            encoder.copy_texture_to_buffer(
                src.texture.as_image_copy(),
                TexelCopyBufferInfo {
                    buffer: &copier.buffer,
                    layout: TexelCopyBufferLayout {
                        offset:         0,
                        bytes_per_row:  Some(std::num::NonZero::new(padded as u32).unwrap().into()),
                        rows_per_image: None,
                    },
                },
                src.size,
            );

            world.resource::<RenderQueue>().submit(std::iter::once(encoder.finish()));
        }

        Ok(())
    }
}

fn copy_buffer_to_channel(
    copiers:       Res<ImageCopiers>,
    render_device: Res<RenderDevice>,
    sender:        Res<RenderWorldSender>,
) {
    for copier in copiers.iter() {
        if !copier.enabled.load(Ordering::Relaxed) {
            continue;
        }
        let buffer_slice = copier.buffer.slice(..);
        let (s, r)       = crossbeam_channel::bounded(1);
        buffer_slice.map_async(MapMode::Read, move |result| {
            s.send(result.expect("buffer map failed")).ok();
        });
        render_device.poll(Maintain::wait()).panic_on_timeout();
        r.recv().expect("map_async recv failed");
        let _ = sender.0.send(buffer_slice.get_mapped_range().to_vec());
        copier.buffer.unmap();
    }
}
