# Optimización del pipeline de grabación (`--record`)

Registro del trabajo de optimización del pipeline headless GPU→ffmpeg de `src/plugins/record.rs`.
Objetivo: aprovechar el M4 Pro al 100% para acelerar la generación de video.

**Hardware de referencia:** Apple M4 Pro — 12 cores (8P+4E), 24 GiB, 2 motores de codificación de video.

## Estado de las ramas

Todas las cifras medidas **con cargador (AC power)** salvo aviso. Demo = standalone (60s);
canicas 60s = flujo real del usuario.

| Rama | Contenido | Demo | Canicas (flujo real) |
|------|-----------|------|----------------------|
| `master` | baseline (timing determinista ManualDuration) | 3.3x | 2.8x |
| `perf/record-pipeline` | Fases 1-3: libx264 + hilo escritor + readback async + crop | 7.8x | 5.2x |
| `perf/yuv-gpu` | Fase 4: + conversión RGBA→yuv420p en GPU (compute) | **9.5x** | ~5.0x |
| `perf/yuv-cpu` | (descartada) conversión en CPU con rayon — PEOR, ver abajo | 7.3x | 4.6x |
| `perf/encode-parallel` | sobre yuv-gpu: N libx264 en paralelo + concat | 8.6x* | **5.76x** |

`*` el demo es render-bound, no encode-bound → el encode paralelo no ayuda ahí (solo añade
contención); su valor está en canicas (encode-bound). `perf/encode-parallel` es la rama de
cabecera. **Sin mergear a master** (decisión pendiente).

Medidas: "demo" = binario standalone `rapier-bevy` (escena ligera). "canicas" = `canicasbrawl-rapier`
(escena real, pesada, más GPU-bound). xRealtime = segundos de video / segundos de tiempo. El log
`STEADY-STATE` mide solo la CAPTURA (excluye arranque); con encode paralelo eso miente (queda
encode en cola) → usar el `TOTAL` (captura+encode+concat, sin arranque) que ahora imprime el binario.

## ⚡ Recomendación: grabar SIEMPRE con el cargador conectado

En MacBook, batería vs AC power cambia el techo de CPU/GPU (macOS limita el power en batería).
Observado al medir: conectar el cargador subió el techo NULL del demo (GPU) de ~9.6x a **12.45x**
y el de canicas de ~7.8x a **9.63x**. **Para producir video con `--record`, enchufá la compu** —
es speedup gratis. Y para comparar variantes, medí siempre en el mismo estado de energía
(`pmset -g batt` debe decir `AC Power`), o los números no son comparables.

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

## La variante CPU: MEDIDA Y DESCARTADA (2026-06-06)

La hipótesis prioritaria era que convertir yuv **en CPU** (cores ociosos) sería más rápido que
en GPU. Se implementó en la rama **`perf/yuv-cpu`** (readback RGBA sin compute + conversión
RGBA→I420 con rayon, BT.601 idéntico al shader; flags `RECORD_CONV_THREADS`/`RECORD_X264_THREADS`)
y se midió contra `perf/yuv-gpu` **en las mismas condiciones (con cargador, AC power)**.

**Resultado: la variante CPU es PEOR en ambas escenas.**

| Escena  | Variante     | Techo (NULL) | Completo (mejor reparto) |
|---------|--------------|--------------|--------------------------|
| Demo    | CPU (rayon)  | 10.0x        | 7.28x (conv6/x264·6)     |
| Demo    | GPU (compute)| **12.45x**   | **9.48x**                |
| Canicas | CPU (rayon)  | 9.0x         | 4.63x (conv4/x264·6)     |
| Canicas | GPU (compute)| **9.63x**    | **5.70x**                |

Barrer el reparto conv/x264 (2..8 vs 4..8) casi no movió la aguja (demo 6.9–7.5x): el cuello no
es la conversión ni el encode, es estructural.

### Por qué pierde (causa raíz: ancho de banda en memoria unificada)
- **El techo NULL ya es menor con readback RGBA** (demo 10.0x vs 12.45x del I420; canicas 9.0 vs
  9.63). En Apple Silicon la memoria es unificada: mover **8.3 MB RGBA/frame** por la CPU
  (readback + `to_vec` + lectura en la conversión) compite por el MISMO ancho de banda que usa
  el render de la GPU. El compute GPU mueve **2.7x menos bytes** (3.1 MB I420) y mantiene la
  conversión on-chip → menos presión de BW → techo más alto.
- El gap NULL→completo en CPU es grande (demo 10→7.3, canicas 9→4.6): la conversión y el
  `to_vec` de 8.3 MB pelean CPU+BW con render y libx264.

### El "techo de 15.6x" era un ARTEFACTO
Las mediciones originales de "techo de Bevy = 15.6x" en el demo se tomaron **cuando la cámara
aún NO apuntaba al OffscreenTarget** (grababa negro → render casi vacío). Al corregir la cámara
(commit 7f742f9), el render real del demo topa a **~12.5x** (readback I420) / ~10x (readback RGBA).
Por eso "~11x en CPU" era imposible: estaba por encima del techo real del render. **No existían
6.6 cores ociosos esperando trabajo útil**; el pipeline ya estaba cerca de su techo real.

### Veredicto
**`perf/yuv-gpu` (compute) supera a la CPU** en ambas escenas. `perf/yuv-cpu` se conserva como
registro del experimento (resultado negativo, bien medido).

## Encode en chunks paralelos (`perf/encode-parallel`) — IMPLEMENTADO

Sobre `perf/yuv-gpu`. Un solo libx264 no sigue el ritmo del render en canicas (encode-bound), así
que el dispatcher corta el stream I420 en segmentos temporales (`RECORD_SEG_FRAMES`) y los reparte
a un pool de `RECORD_SEGMENTS` encoders libx264 en paralelo; cada segmento es un .mp4 autocontenido
que al final se concatena con `-c copy` (sin re-encode, instantáneo, corte exacto por keyframe).

**Resultado (canicas 60s, AC, mejor config `RECORD_SEGMENTS=6 RECORD_SEG_FRAMES=90 RECORD_X264_THREADS=2`):**
- single-encoder (yuv-gpu): ~5.0x sostenido.
- encode-parallel: **captura 6.02x, TOTAL 5.76x** → **~15% más rápido** sostenido.

Correctness validada: 1800/3600 frames exactos, 30/60s, yuv420p, YAVG idéntico al single-encoder.

**Por qué la ganancia es modesta (no el salto a ~9x):** a 60s sostenidos los N encoders compiten
con render+compute+readback por los 12 cores. El techo de canicas (9.63x NULL) lo pone la GPU
(render de la escena + compute), no el encode. El encode paralelo cierra el gap encode→techo pero
no sube el techo. En el DEMO (render-bound) no ayuda: solo añade contención (8.6x vs 9.5x single).

**Cuándo usarlo:** escenas encode-bound (mucha CPU libre, render rápido). Para canicas, sí. Flags:
`RECORD_SEGMENTS` (def 4), `RECORD_SEG_FRAMES` (def 120), `RECORD_X264_THREADS` por encoder (def 3).

## Qué queda por intentar (el cuello real ahora)

Con la conversión ya resuelta vía GPU, los techos dicen dónde está el cuello de cada escena:
- **Demo (render+compute bound):** completo 9.48x vs techo 12.45x. El gap es render/compute en la
  GPU. Candidato: **async compute** — solapar el compute de conversión con el render del frame
  siguiente (segunda queue Metal). Si se elimina la serialización render→compute, el techo podría
  subir. Riesgo: wgpu/Metal lo soportan parcialmente; complejo.
- **Canicas (encode bound):** completo 5.70x vs techo 9.63x. El gap grande es libx264/pipe, NO el
  render. Candidato: **encode en chunks temporales paralelos** (varios libx264 → concat). El
  encode software SÍ escala con cores. O `RECORD_PRESET=ultrafast`. También optimizar la ESCENA
  (sombras, draw calls) subiría el techo NULL.
- **NV12 en vez de I420**: mismo tamaño (3.1 MB), un solo plano de croma; podría simplificar el
  shader y algunos paths de encode lo prefieren.

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
