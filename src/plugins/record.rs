use bevy::{
    prelude::*,
    time::TimeUpdateStrategy,
    render::{
        Extract, Render, RenderApp, RenderSet,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel},
        render_resource::{
            BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, Buffer, BufferDescriptor,
            BufferUsages, CachedComputePipelineId, ComputePassDescriptor,
            ComputePipelineDescriptor, Extent3d, Maintain, MapMode, PipelineCache, Shader,
            ShaderStages, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
            TextureViewDescriptor,
            binding_types::{storage_buffer, texture_2d},
        },
        renderer::{RenderContext, RenderDevice},
    },
    winit::WinitPlugin,
};
use crossbeam_channel::{Receiver, Sender};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const WIDTH:    u32 = 1080;
const HEIGHT:   u32 = 1920;
const FPS:      u32 = 60;

/// Tamaño de un frame I420 (yuv420p): Y a resolución plena + U y V a 1/4. Lo que el
/// compute shader produce y se lee de vuelta — 2.7x menos que RGBA, sin padding de fila.
const YUV_BYTES: usize = (WIDTH as usize * HEIGHT as usize) * 3 / 2;

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
    frame_tx:        Option<Sender<Vec<u8>>>, // frames I420 → dispatcher (segmenta y reparte a N encoders)
    writer:          Option<JoinHandle<()>>,  // dispatcher: pool de encoders + concat final
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

        // El encoder por software libx264 satura los 12 cores del M4 y alcanza ~15-20x,
        // muy por encima del encoder de hardware (h264_videotoolbox topa a ~4x: el bloque
        // de codificación es un recurso compartido que no escala). veryfast da el mejor
        // balance calidad/tamaño; RECORD_PRESET=ultrafast cuando se necesita más throughput.
        let preset = std::env::var("RECORD_PRESET").unwrap_or_else(|_| "veryfast".into());
        // Threads por encoder. En modo paralelo corren varios libx264 a la vez, así que
        // cada uno usa pocos: default 3 → con RECORD_SEGMENTS=4 reparte ~12 cores. RECORD_X264_THREADS.
        let x264_threads = std::env::var("RECORD_X264_THREADS").unwrap_or_else(|_| "3".into());
        // Nº de encoders libx264 en paralelo. El render produce frames más rápido (techo ~9.6x)
        // de lo que un solo libx264 los consume (~5.7x); con N encoders sobre segmentos
        // temporales distintos el encode deja de ser el cuello. RECORD_SEGMENTS.
        let segments: usize = std::env::var("RECORD_SEGMENTS")
            .ok().and_then(|v| v.parse().ok()).filter(|&n| n > 0).unwrap_or(4);
        // Frames por segmento; cada segmento es un .mp4 que luego se concatena (-c copy).
        // Chico = más paralelismo y archivos; grande = menos overhead y más RAM. RECORD_SEG_FRAMES.
        let seg_frames: usize = std::env::var("RECORD_SEG_FRAMES")
            .ok().and_then(|v| v.parse().ok()).filter(|&n| n > 0).unwrap_or(120);

        let null_sink = std::env::var("RECORD_NULL").is_ok();

        // Channel render→dispatcher. bounded da backpressure natural: si los encoders se
        // atrasan, el render loop frena en vez de acumular GB de RAM.
        let (frame_tx, frame_rx) = crossbeam_channel::bounded::<Vec<u8>>(8);

        let (frame_tx, writer) = if null_sink {
            (None, None)
        } else {
            let out_path = output_path.clone();
            // Dispatcher: acumula los frames I420 (3.1 MB, ya convertidos en GPU) en segmentos
            // de seg_frames y los reparte a un pool de `segments` encoders libx264. Al cerrarse
            // el channel espera a todos y concatena los .mp4 parciales en el archivo final.
            let writer = std::thread::spawn(move || {
                run_dispatcher(frame_rx, segments, seg_frames, preset, x264_threads, out_path);
            });
            (Some(frame_tx), Some(writer))
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
                frame_tx,
                writer,
                null_sink,
                t_first: None,
            })
            .add_systems(PreStartup, create_offscreen_target)
            .add_systems(FixedUpdate, mark_capture)
            .add_systems(Update, (receive_and_pipe, check_complete).chain());

        // Shader embebido en el binario (no depende del asset path del juego host).
        let shader = app
            .world_mut()
            .resource_mut::<Assets<Shader>>()
            .add(Shader::from_wgsl(
                include_str!("rgba_to_yuv420p.wgsl"),
                "rapier_bevy/rgba_to_yuv420p.wgsl",
            ));

        let render_app = app.sub_app_mut(RenderApp);
        let mut graph  = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(ImageCopyLabel, ImageCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, ImageCopyLabel);

        render_app
            .insert_resource(RenderWorldSender(sender))
            .insert_resource(YuvShader(shader))
            .init_resource::<ReadbackQueue>()
            .add_systems(ExtractSchedule, image_copy_extract)
            .add_systems(
                Render,
                (
                    (ensure_yuv_resources, release_buffers).in_set(RenderSet::Prepare),
                    map_buffers.after(RenderSet::Render),
                ),
            );
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
    // Permite enlazar la textura sRGB con un view no-sRGB en el compute (lee bytes gamma
    // sin conversión). El formato base se resuelve a Rgba8UnormSrgb por bevy_default().
    render_target.texture_descriptor.view_formats = &[TextureFormat::Rgba8Unorm];

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

    // Cerrar el channel hace que el dispatcher lea el último frame, despache el segmento
    // parcial, espere a todos los encoders y concatene los .mp4 en el archivo final.
    drop(state.frame_tx.take());
    println!("[record] {} frames capturados, encode + concat…", state.frames_captured);
    if let Some(writer) = state.writer.take() {
        let _ = writer.join();
    }

    // Con encode paralelo el STEADY-STATE (solo captura) miente: al terminar de capturar
    // quedan segmentos encodeando. Este TOTAL (captura + tail de encode + concat, sin el
    // arranque) es el número honesto de cuánto tarda en quedar el mp4.
    if let Some(t0) = state.t_first {
        let secs = t0.elapsed().as_secs_f64();
        let video_secs = state.total_frames as f64 / FPS as f64;
        println!(
            "[record] TOTAL (captura+encode+concat, sin arranque): {:.2}s → {:.2}x realtime",
            secs, video_secs / secs,
        );
    }
    println!("[record] {} ready", state.output_path.display());

    exit.write(AppExit::Success);
}

// ── Encode paralelo: dispatcher → pool de N libx264 → concat ──────────────
//
// Un solo libx264 no sigue el ritmo del render (techo ~9.6x vs encode ~5.7x), así que los
// frames se acumulan. El dispatcher los corta en segmentos temporales y los reparte a N
// encoders que corren en paralelo (el encode software SÍ escala con cores); cada segmento
// es un .mp4 autocontenido que al final se concatena sin re-encode (-c copy, instantáneo).

/// Acumula frames I420 en segmentos de `seg_frames` y los despacha a `segments` encoders.
/// Al cerrarse `frame_rx`, despacha el segmento parcial, espera a los encoders y concatena.
fn run_dispatcher(
    frame_rx:   Receiver<Vec<u8>>,
    segments:   usize,
    seg_frames: usize,
    preset:     String,
    x264:       String,
    output:     PathBuf,
) {
    let dir  = output.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let stem = output.file_stem().and_then(|s| s.to_str()).unwrap_or("record").to_string();

    // Pool: cada worker toma (idx, buffer I420) y produce {stem}.segNNNN.mp4. El channel
    // bounded acota cuántos segmentos viven en RAM a la vez (backpressure → frena el render).
    let (seg_tx, seg_rx) = crossbeam_channel::bounded::<(usize, Vec<u8>)>(segments);
    let workers: Vec<JoinHandle<()>> = (0..segments)
        .map(|_| {
            let seg_rx = seg_rx.clone();
            let preset = preset.clone();
            let x264   = x264.clone();
            let dir    = dir.clone();
            let stem   = stem.clone();
            std::thread::spawn(move || {
                for (idx, buf) in seg_rx.iter() {
                    encode_segment(&dir, &stem, idx, &buf, &preset, &x264);
                }
            })
        })
        .collect();
    drop(seg_rx);

    let mut buf    = Vec::with_capacity(seg_frames * YUV_BYTES);
    let mut in_seg = 0usize;
    let mut idx    = 0usize;
    for raw in frame_rx.iter() {
        buf.extend_from_slice(&raw);
        in_seg += 1;
        if in_seg == seg_frames {
            let full = std::mem::replace(&mut buf, Vec::with_capacity(seg_frames * YUV_BYTES));
            let _ = seg_tx.send((idx, full));
            idx += 1;
            in_seg = 0;
        }
    }
    if in_seg > 0 {
        let _ = seg_tx.send((idx, buf));
        idx += 1;
    }
    drop(seg_tx);
    for w in workers { let _ = w.join(); }

    concat_segments(&dir, &stem, idx, &output);
}

/// Codifica un segmento I420 a {stem}.segNNNN.mp4 con su propio libx264. write_all bloquea
/// mientras ffmpeg consume el buffer → este worker queda ocupado todo el encode (lo cual es
/// justo el trabajo que paraleliza con los demás workers sobre otros segmentos).
fn encode_segment(dir: &Path, stem: &str, idx: usize, buf: &[u8], preset: &str, x264: &str) {
    let seg_path = dir.join(format!(".{stem}.seg{idx:04}.mp4"));
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",         "rawvideo",
            "-pix_fmt",   "yuv420p",
            "-s:v",       &format!("{}x{}", WIDTH, HEIGHT),
            "-framerate", &FPS.to_string(),
            "-i",         "pipe:0",
            "-c:v",       "libx264",
            "-preset",    preset,
            "-crf",       "20",
            "-threads",   x264,
            "-pix_fmt",   "yuv420p",
            seg_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("[record] ffmpeg not found — install with: brew install ffmpeg");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(buf); // al salir del scope, stdin se cierra → EOF para ffmpeg
    }
    let _ = child.wait();
}

/// Une los segmentos en orden con el demuxer concat (-c copy, sin re-encode) y limpia los
/// temporales. Cada segmento ya arranca con keyframe, así que el corte es exacto.
fn concat_segments(dir: &Path, stem: &str, count: usize, output: &Path) {
    if count == 0 {
        return;
    }
    let list_path = dir.join(format!(".{stem}.concat.txt"));
    let mut list = String::new();
    for i in 0..count {
        // Rutas relativas al directorio del list file (donde viven los segmentos).
        list.push_str(&format!("file '.{stem}.seg{i:04}.mp4'\n"));
    }
    let _ = std::fs::write(&list_path, &list);

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",     "concat",
            "-safe",  "0",
            "-i",     list_path.to_str().unwrap(),
            "-c",     "copy",
            output.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let _ = std::fs::remove_file(&list_path);
    for i in 0..count {
        let _ = std::fs::remove_file(dir.join(format!(".{stem}.seg{i:04}.mp4")));
    }

    if let Ok(s) = status {
        if !s.success() {
            eprintln!("[record] concat ffmpeg exited with {s}");
        }
    }
}

// ── RenderGraph: GPU texture → CPU buffer → channel ──────────────────────

/// Profundidad del anillo de readback. Con N buffers la GPU puede ir hasta N-1 frames
/// por delante de la CPU: mientras la CPU mapea/copia el frame f-2, la GPU ya renderizó
/// f-1 y f. Sin esto, poll(wait) serializa GPU↔CPU y deja ~7 cores ociosos.
const READBACK_RING: usize = 4;

#[derive(Clone, Component)]
struct ImageCopier {
    buffers:   Arc<Vec<Buffer>>,
    src_image: Handle<Image>,
    frame:     Arc<std::sync::atomic::AtomicUsize>, // qué buffer del anillo toca este frame
    enabled:   Arc<AtomicBool>,
}

impl ImageCopier {
    fn new(src_image: Handle<Image>, _size: Extent3d, device: &RenderDevice) -> Self {
        // Anillo de buffers MAP_READ que reciben el I420 ya convertido (3.1 MB, sin padding).
        let buffers = (0..READBACK_RING)
            .map(|i| {
                device.create_buffer(&BufferDescriptor {
                    label:              Some(&format!("record_readback_{i}")),
                    size:               YUV_BYTES as u64,
                    usage:              BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        ImageCopier {
            buffers: Arc::new(buffers),
            src_image,
            frame:   Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }
}

/// Shader de conversión (handle compartido main↔render world).
#[derive(Resource)]
struct YuvShader(Handle<Shader>);

/// Pipelines de compute (luma y croma) y su bind group layout.
#[derive(Resource)]
struct YuvPipeline {
    layout:      BindGroupLayout,
    pipeline_y:  CachedComputePipelineId,
    pipeline_uv: CachedComputePipelineId,
}

/// Buffer de salida del compute (I420), creado una vez.
#[derive(Resource)]
struct YuvResources {
    storage: Buffer, // STORAGE | COPY_SRC, recibe el I420 del compute
}

/// Crea pipelines y buffer de compute en el primer frame del render world, cuando
/// RenderDevice y PipelineCache ya existen (no están disponibles en build()/finish()
/// porque RecordPlugin añade DefaultPlugins dentro de su propio build).
fn ensure_yuv_resources(
    mut commands: Commands,
    device:       Res<RenderDevice>,
    cache:        Res<PipelineCache>,
    shader:       Res<YuvShader>,
    existing:     Option<Res<YuvPipeline>>,
) {
    if existing.is_some() {
        return;
    }
    let layout = device.create_bind_group_layout(
        "yuv_convert_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                storage_buffer::<Vec<u32>>(false),
            ),
        ),
    );
    let mk = |entry: &str| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("yuv_{entry}").into()),
            layout: vec![layout.clone()],
            push_constant_ranges: vec![],
            shader: shader.0.clone(),
            shader_defs: vec![],
            entry_point: entry.to_string().into(),
            zero_initialize_workgroup_memory: false,
        })
    };
    commands.insert_resource(YuvPipeline {
        pipeline_y:  mk("to_y"),
        pipeline_uv: mk("to_uv"),
        layout,
    });
    commands.insert_resource(YuvResources {
        storage: device.create_buffer(&BufferDescriptor {
            label:              Some("yuv_storage"),
            size:               YUV_BYTES as u64,
            usage:              BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }),
    });
}

/// Cola de buffers cuyo map_async se lanzó pero aún no se recogió (en el render world).
#[derive(Resource, Default)]
struct ReadbackQueue {
    in_flight: std::collections::VecDeque<InFlight>,
}

struct InFlight {
    idx: usize,
    rx:  Receiver<bool>, // el callback de map_async manda ok/err aquí
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
        let cache      = world.resource::<PipelineCache>();

        // Recursos de compute creados lazily por ensure_yuv_resources; los primeros
        // frames pueden no tenerlos todavía.
        let (Some(yuv), Some(yuv_res)) = (
            world.get_resource::<YuvPipeline>(),
            world.get_resource::<YuvResources>(),
        ) else {
            return Ok(());
        };

        // Ambos pipelines deben estar compilados; si no, salta el frame (los primeros
        // frames se descartan de todos modos por frames_to_skip).
        let (Some(pl_y), Some(pl_uv)) = (
            cache.get_compute_pipeline(yuv.pipeline_y),
            cache.get_compute_pipeline(yuv.pipeline_uv),
        ) else {
            return Ok(());
        };

        for copier in copiers.iter() {
            let Some(src) = gpu_images.get(&copier.src_image) else { continue };
            let k = copier.frame.load(Ordering::Relaxed) % READBACK_RING;

            // View no-sRGB sobre la textura del render: el shader lee los bytes gamma
            // directamente (sin conversión sRGB→linear, que cuesta un pow por canal).
            let gamma_view = src.texture.create_view(&TextureViewDescriptor {
                label:  Some("yuv_src_unorm"),
                format: Some(TextureFormat::Rgba8Unorm),
                ..default()
            });

            let bind_group = render_context.render_device().create_bind_group(
                "yuv_convert_bg",
                &yuv.layout,
                &BindGroupEntries::sequential((
                    &gamma_view,
                    yuv_res.storage.as_entire_binding(),
                )),
            );

            // Conversión RGBA → I420 en la GPU, fundida en el encoder del frame.
            {
                let mut pass = render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor::default());
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_pipeline(pl_y);
                pass.dispatch_workgroups((WIDTH / 4).div_ceil(8), HEIGHT.div_ceil(8), 1);
                pass.set_pipeline(pl_uv);
                pass.dispatch_workgroups((WIDTH / 8).div_ceil(8), (HEIGHT / 2).div_ceil(8), 1);
            }

            // El I420 resultante (3.1 MB) va al buffer del anillo para readback async.
            render_context.command_encoder().copy_buffer_to_buffer(
                &yuv_res.storage,
                0,
                &copier.buffers[k],
                0,
                YUV_BYTES as u64,
            );
        }

        Ok(())
    }
}

/// Corre ANTES del node (RenderSet::Prepare): recoge los frames cuyo readback completó
/// y, sobre todo, garantiza que buffers[k] esté DESMAPEADO antes de que el node lo
/// reescriba este frame (un copy a un buffer mapeado es ilegal en wgpu).
fn release_buffers(
    copiers:       Res<ImageCopiers>,
    render_device: Res<RenderDevice>,
    sender:        Res<RenderWorldSender>,
    mut queue:     ResMut<ReadbackQueue>,
) {
    for copier in copiers.iter() {
        if !copier.enabled.load(Ordering::Relaxed) {
            continue;
        }
        let k = copier.frame.load(Ordering::Relaxed) % READBACK_RING;

        // Bombea callbacks completados y recoge en orden FIFO (no bloquea).
        render_device.poll(Maintain::Poll);
        drain_ready(&mut queue, copier, &sender);

        // Si buffers[k] (el que el node usará) sigue mapeado, drénalo bloqueando.
        // Solo ocurre si la GPU se atrasó; normalmente drain_ready ya lo recogió.
        while queue.in_flight.iter().any(|f| f.idx == k) {
            render_device.poll(Maintain::wait()).panic_on_timeout();
            drain_one_blocking(&mut queue, copier, &sender);
        }
    }
}

/// Corre DESPUÉS del node (after RenderSet::Render): el node ya copió el I420 a
/// buffers[k] en este submit, así que lanzamos su map_async (asíncrono) y avanzamos
/// el anillo. El resultado se recoge en release_buffers de un frame posterior.
fn map_buffers(copiers: Res<ImageCopiers>, mut queue: ResMut<ReadbackQueue>) {
    for copier in copiers.iter() {
        if !copier.enabled.load(Ordering::Relaxed) {
            continue;
        }
        let k = copier.frame.load(Ordering::Relaxed) % READBACK_RING;

        let (s, r) = crossbeam_channel::bounded(1);
        copier.buffers[k].slice(..).map_async(MapMode::Read, move |res| {
            let _ = s.send(res.is_ok());
        });
        queue.in_flight.push_back(InFlight { idx: k, rx: r });

        copier.frame.fetch_add(1, Ordering::Relaxed);
    }
}

/// Recoge del frente todos los buffers cuyo map ya completó, sin bloquear.
fn drain_ready(queue: &mut ReadbackQueue, copier: &ImageCopier, sender: &RenderWorldSender) {
    while let Some(front) = queue.in_flight.front() {
        match front.rx.try_recv() {
            Ok(true) => {
                let idx = front.idx;
                let _ = sender.0.send(copier.buffers[idx].slice(..).get_mapped_range().to_vec());
                copier.buffers[idx].unmap();
                queue.in_flight.pop_front();
            }
            Ok(false) => { queue.in_flight.pop_front(); } // map fallido: descarta sin leer
            Err(_) => break,                              // el frente aún no está listo
        }
    }
}

/// Recoge exactamente el frente, asumiendo que su map ya completó (tras poll(wait)).
fn drain_one_blocking(queue: &mut ReadbackQueue, copier: &ImageCopier, sender: &RenderWorldSender) {
    if let Some(front) = queue.in_flight.pop_front() {
        if front.rx.recv().unwrap_or(false) {
            let _ = sender.0.send(copier.buffers[front.idx].slice(..).get_mapped_range().to_vec());
            copier.buffers[front.idx].unmap();
        }
    }
}
