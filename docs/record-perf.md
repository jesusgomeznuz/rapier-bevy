# Optimización del pipeline de grabación (`--record`)

Registro del trabajo de optimización del pipeline headless GPU→ffmpeg de `src/plugins/record.rs`.
Objetivo: aprovechar el M4 Pro al 100% para acelerar la generación de video.

**Hardware de referencia:** Apple M4 Pro — 12 cores (8P+4E), 24 GiB, 2 motores de codificación de video.

## Estado de las ramas

| Rama | Contenido | Demo | Canicas (flujo real) |
|------|-----------|------|----------------------|
| `master` | baseline (timing determinista ManualDuration) | 3.3x | **2.8x** |
| `perf/record-pipeline` | Fases 1-3: libx264 + hilo escritor + readback async + crop | 7.8x | 5.2x |
| `perf/yuv-gpu` | Fase 4: + conversión RGBA→yuv420p en GPU (compute) | **9.0x** | **5.8x** |

`perf/yuv-gpu` es la rama de cabecera con todo el trabajo. **Aún sin mergear a master** (decisión pendiente).

Medidas: "demo" = binario standalone `rapier-bevy` (escena ligera). "canicas" = `canicasbrawl-rapier`
(escena real, pesada, más GPU-bound). xRealtime = segundos de video / segundos de wall-clock.
"steady-state" excluye arranque/carga de assets; "wall-clock" lo incluye.

## Cómo medir / flags

```bash
# steady-state (loga "STEADY-STATE: ... Nx realtime")
cargo run --release -- --record 30

# techo de Bevy (descarta frames, sin encode) — mide render+readback puros
RECORD_NULL=1 cargo run --release -- --record 30

# tuning
RECORD_PRESET=ultrafast   cargo run --release -- --record 30   # encoder más rápido (def veryfast)
RECORD_X264_THREADS=10    cargo run --release -- --record 30   # threads de libx264 (def 6)

# correr el binario directo (canicasbrawl usa target-dir compartido = rapier-bevy/target):
#   los assets de Bevy en release se buscan junto al ejecutable → usar BEVY_ASSET_ROOT
BEVY_ASSET_ROOT=/Users/jesus/canicasbrawl-rapier \
  /Users/jesus/rapier-bevy/target/release/canicasbrawl-rapier --record 30
```

Verificar contenido/color del mp4 (el demo standalone grababa negro porque su cámara no
apuntaba al OffscreenTarget — ya corregido en main.rs; canicasbrawl sí lo conectaba):
```bash
ffprobe -v error -show_entries format=duration:stream=width,height,nb_frames,pix_fmt \
  -of default=noprint_wrappers=1 outputs/record_30s.mp4
# luminancia media (0=negro): ~16 = vacío, ~77/175 = con contenido
ffmpeg -ss 5 -i out.mp4 -frames:v 1 -vf signalstats,metadata=print:key=lavfi.signalstats.YAVG -f null -
```

## Diagnóstico: el cuello se movió 4 veces

1. **Encoder hardware (`h264_videotoolbox`) topa a ~240fps / 4x** a 1080×1920 y **NO escala**
   con procesos paralelos (el bloque de codificación es recurso compartido). Medido: N=1,2,3,4
   encoders → siempre ~240fps agregados. → **Cambiado a `libx264` (software)**: satura los 12
   cores y da ~15x (veryfast) / ~20x (ultrafast) aislado, 5x más que el HW.

2. **Serialización GPU↔CPU**: el `poll(Maintain::wait())` bloqueaba el render thread cada frame,
   dejando ~7 cores ociosos. → **Anillo de 4 buffers con `map_async` + `poll(Poll)` no bloqueante**
   (la GPU va hasta 3 frames por delante). Techo de Bevy: 454fps → 938fps (15.6x) en el demo.
   También: encode movido a **hilo escritor dedicado** (desacopla ffmpeg del render loop).

3. **Volumen de datos por el pipe a ffmpeg**: experimento de 4 puntos (A=Bevy solo 15.4x,
   D=+channel+write a /dev/null 15.8x → transporte interno GRATIS, B=+pipe a ffmpeg 9.3x →
   AQUÍ cae, C=+encode 7.8x). El cuello era ffmpeg demuxeando RGBA (8.3MB/frame) del pipe.
   Confirmado: leer NV12 (3.1MB) del pipe es 2.7x más rápido que RGBA. El `crop` mueve el
   strip de padding wgpu a ffmpeg (gratis). → **Conversión a yuv420p en GPU** (Fase 4):
   readback de 3.1MB en vez de 8.3MB, ffmpeg sin swscale ni crop.

4. **Render + compute en la GPU** (estado actual): el compute shader de conversión **bajó el
   techo de Bevy de 15.6x a 9.4x** porque las barreras render→compute→copy se serializan en la
   GPU. El pipeline completo (9.0x demo) ya casi toca ese techo → el pipe dejó de ser el cuello.

## El hallazgo clave para seguir (PRIORIDAD)

**Sospecha fuerte: la conversión yuv en CPU sería MÁS rápida que en GPU en estas escenas.**

- El pipeline es **GPU-bound** (demo: ~5.4 de 12 cores usados → 6.6 cores OCIOSOS).
- El compute GPU resuelve el cuello del pipe **pero le roba trabajo al render** (techo 15.6→9.4x).
- El experimento `RECORD_HALFPIPE` (mandar 3.1MB/frame al pipe SIN el costo del compute) dio
  **~11x en el demo** — el techo si reducimos el pipe sin tocar la GPU.
- Conversión RGBA→I420 en CPU (en los cores ociosos, en el hilo escritor o un pool): mantiene
  el readback RGBA (techo Bevy 15.6x, no es cuello) y reduce el pipe 2.7x. Cuello esperado:
  el render (15.6x) o la conversión CPU. **Potencial ~11x demo** vs 9x del GPU.

### Plan para la variante CPU
1. En el hilo escritor (o un pool de 2-4 hilos para repartir), convertir el frame RGBA (con
   padding) a I420 antes del `write_all`. Usar un crate SIMD (`yuvutils-rs` o `dcv-color-primitives`,
   ambos con NEON) o a mano con NEON. **Cuidado con el color**: BT.601 limited range, y los bytes
   RGBA del framebuffer ya están en gamma (NO linearizar) — igual que hace el shader actual y swscale.
2. Volver al readback RGBA (revertir el compute) + ffmpeg con `-pix_fmt yuv420p` recibiendo el I420 ya hecho.
3. Medir `RECORD_NULL` (debe volver a ~15.6x) y completo. Comparar contra yuv GPU (9x).
4. Repartir cores: barrer nº de hilos de conversión vs `RECORD_X264_THREADS` (suma ≤ 12).

### Otras ideas no exploradas (intentarlo TODO)
- **Reducir las barreras del compute GPU**: investigar si el compute puede solaparse con el
  render del frame siguiente (async compute / segunda queue). wgpu/Metal lo soporta parcialmente.
  Si se elimina la serialización render→compute, el techo GPU podría volver hacia 15x.
- **NV12 en vez de I420**: videotoolbox/algunos paths prefieren NV12 (Y plano + UV intercalado).
  Mismo tamaño (3.1MB); podría simplificar el shader (un solo plano de croma) y el empaquetado.
- **Encodear en chunks temporales con varios `libx264` en paralelo**: el encode software SÍ
  escala con cores (a diferencia del HW). Si tras la conversión CPU el encode vuelve a ser cuello,
  partir el stream en N segmentos → N procesos → concat. Ojo: la conversión CPU y los N encoders
  competirían por los 12 cores; medir el reparto óptimo.
- **Escena de canicas es GPU-bound a 8.93x** (su `RECORD_NULL`): ahí el techo lo pone el render
  mismo (sombras, muchos sprites). Para subir canicas habría que optimizar la ESCENA (no el
  pipeline): revisar shadow maps, draw calls, materiales. El pipeline ya casi no es el cuello ahí.
- **Bajar la latencia de arranque**: el wall-clock incluye carga de assets (GLB, PNGs). En videos
  cortos pesa; para 30s el arranque (~1.5s) diluye el speedup. Precargar/streamear assets ayudaría.

## Arquitectura actual (perf/yuv-gpu)

```
Render escena → textura RGBA (offscreen, sRGB)
  → [compute] rgba_to_yuv420p.wgsl: view Rgba8Unorm (bytes gamma, sin pow) → storage buffer I420
  → copy_buffer_to_buffer → anillo[k] (MAP_READ, 3.1MB)
  → [release_buffers @ Prepare] garantiza buffers[k] desmapeado antes del node
  → [map_buffers @ post-Render] map_async(buffers[k]) → cola FIFO
  → RenderWorldSender → MainWorldReceiver → hilo escritor (BufWriter) → ffmpeg stdin
  → libx264 veryfast crf20 -pix_fmt yuv420p → outputs/record_Ns.mp4
```

Notas de correctness:
- **Color**: BT.601 limited range en el shader. Leer la textura sRGB vía un view `Rgba8Unorm`
  (requiere `view_formats` en el descriptor del target) da los bytes gamma directos → idéntico
  a lo que swscale recibía. YAVG del video = idéntico al pipeline RGBA previo (validado).
- **Anillo**: la liberación (desmapeo) de `buffers[k]` DEBE ocurrir antes de que el node lo
  reescriba (`release_buffers` en Prepare). Hacerlo después (como estaba) da
  "Buffer still mapped" en `Queue::submit`. El frame counter lo incrementa `map_buffers`.
- **Recursos de compute** se crean lazy en `ensure_yuv_resources` (sistema en Prepare), NO en
  `build()`/`finish()`: RecordPlugin añade DefaultPlugins dentro de su propio build, así que
  RenderDevice/PipelineCache no existen en finish() de este plugin.
- **Timing 1:1**: `Time::<Fixed>` 60Hz + `ManualDuration(1/60)` → 1 step de física = 1 frame de
  video, determinista (engine.rs).
- El shader hardcodea WIDTH=1080/HEIGHT=1920 (van embebidos y especializados); si cambian en
  record.rs, actualizar el .wgsl.

## Comparativa de encoders (contenido real, 1800 frames, aislado)

| Encoder | xRealtime | Notas |
|---------|-----------|-------|
| h264_videotoolbox (HW) | 4.0x | no escala con procesos paralelos |
| libx264 veryfast crf20 | 15.7x | mejor calidad/tamaño, **elegido** |
| libx264 ultrafast crf20 | 20.7x | más rápido, archivo algo mayor (irrelevante) |
| libx264 faster crf20 | 11.4x | |

Para esta escena los archivos son diminutos (0.1–1.1 MB / 30s) con cualquier preset.
