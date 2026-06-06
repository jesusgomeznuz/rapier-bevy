use bevy::{
    prelude::*,
    time::TimeUpdateStrategy,
    render::{
        Extract, Render, RenderApp, RenderSet,
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
    io::{BufWriter, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const WIDTH:    u32 = 1080;
const HEIGHT:   u32 = 1920;
const FPS:      u32 = 60;

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
    frame_tx:        Option<Sender<Vec<u8>>>, // frames padded → hilo escritor (desacopla encode del render loop)
    writer:          Option<JoinHandle<()>>,
    null_sink:       bool,        // RECORD_NULL=1 → descarta frames, mide techo de Bevy
    t_first:         Option<Instant>, // wall-clock del primer frame piped (excluye arranque)
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

        let pix_fmt = "rgba"; // offscreen Image usa TextureFormat::bevy_default() = RGBA en todas las plataformas

        // El encoder por software libx264 satura los 12 cores del M4 y alcanza ~15-20x,
        // muy por encima del encoder de hardware (h264_videotoolbox topa a ~4x: el bloque
        // de codificación es un recurso compartido que no escala). veryfast da el mejor
        // balance calidad/tamaño; RECORD_PRESET=ultrafast cuando se necesita más throughput.
        let preset = std::env::var("RECORD_PRESET").unwrap_or_else(|_| "veryfast".into());

        let null_sink = std::env::var("RECORD_NULL").is_ok();

        // Channel main world → hilo escritor. bounded da backpressure natural: si el
        // encode se atrasara, el render loop frena en vez de acumular GB de RAM.
        let (frame_tx, frame_rx) = crossbeam_channel::bounded::<Vec<u8>>(8);

        let (ffmpeg_opt, frame_tx, writer) = if null_sink {
            (None, None, None)
        } else {
            let mut ffmpeg = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",         "rawvideo",
                    "-pix_fmt",   pix_fmt,
                    "-s:v",       &format!("{}x{}", WIDTH, HEIGHT),
                    "-framerate", &FPS.to_string(),
                    "-i",         "pipe:0",
                    "-c:v",       "libx264",
                    "-preset",    &preset,
                    "-crf",       "20",
                    "-pix_fmt",   "yuv420p",
                    output_path.to_str().unwrap(),
                ])
                .stdin(Stdio::piped())
                .spawn()
                .expect("[record] ffmpeg not found — install with: brew install ffmpeg");
            let stdin = ffmpeg.stdin.take().expect("ffmpeg stdin");

            // Hilo dedicado: recibe frames padded, quita el padding de wgpu y los pipea a
            // ffmpeg. Saca el strip (1920 filas) y el write bloqueante del loop de render,
            // que así corre a su techo sin esperar al encode.
            let writer = std::thread::spawn(move || {
                let mut out = BufWriter::with_capacity(row_bytes * HEIGHT as usize, stdin);
                for raw in frame_rx.iter() {
                    if row_bytes == padded_bytes {
                        let _ = out.write_all(&raw);
                    } else {
                        for row in raw.chunks(padded_bytes) {
                            let _ = out.write_all(&row[..row_bytes.min(row.len())]);
                        }
                    }
                }
                let _ = out.flush();
            });

            (Some(ffmpeg), Some(frame_tx), Some(writer))
        };

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

        // Cada frame de la app avanza un step fijo de física (1/60 s) → exactamente
        // 1 frame de video por step. La aceleración de producción viene del loop
        // headless (run_loop ZERO) corriendo a cientos de fps, no de saltarse steps:
        // así 60 s de video = 3600 frames = 3600 steps = 60 s de simulación.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f64(1.0 / FPS as f64),
        ));

        app
            .insert_resource(MainWorldReceiver(receiver))
            .insert_resource(AssetsLoading::default())
            .insert_resource(RecordState {
                frames_captured: 0,
                total_frames,
                frames_to_skip:  3,
                capture_pending: false,
                finalized: false,
                output_path,
                ffmpeg_child: ffmpeg_opt,
                frame_tx,
                writer,
                null_sink,
                t_first: None,
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

    if state.t_first.is_none() {
        state.t_first = Some(Instant::now());
    }

    // Entrega el frame crudo (padded) al hilo escritor; el strip + write a ffmpeg
    // ocurre fuera del loop de render. send() solo bloquea si el encode se atrasa (backpressure).
    if let Some(ref tx) = state.frame_tx {
        let _ = tx.send(raw);
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

    if let Some(t0) = state.t_first {
        let secs = t0.elapsed().as_secs_f64();
        let fps  = state.frames_captured as f64 / secs;
        println!(
            "[record] STEADY-STATE: {} frames en {:.2}s → {:.0} fps efectivos → {:.2}x realtime{}",
            state.frames_captured, secs, fps, fps / FPS as f64,
            if state.null_sink { " (NULL SINK — techo de Bevy)" } else { "" },
        );
    }

    // Cerrar el channel hace que el hilo escritor termine su loop, vacíe el BufWriter
    // y cierre stdin de ffmpeg (EOF). Luego esperamos a que drene los frames en vuelo.
    drop(state.frame_tx.take());
    if let Some(writer) = state.writer.take() {
        let _ = writer.join();
    }
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
