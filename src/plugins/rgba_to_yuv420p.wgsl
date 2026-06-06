// Convierte el render RGBA (offscreen) a I420 (yuv420p planar) directamente en la GPU.
// El resultado (3.1 MB) se lee de vuelta en vez de los 8.3 MB de RGBA: 2.7x menos tráfico
// GPU→CPU y por el pipe a ffmpeg, que recibe yuv420p sin swscale ni crop.
//
// Layout del buffer de salida (array<u32>, little-endian, cada u32 = 4 bytes empaquetados):
//   [ plano Y ][ plano U ][ plano V ]   contiguo, sin padding de fila.

// Dimensiones fijas del render offscreen (ver WIDTH/HEIGHT en record.rs). Si cambian
// allí, actualizar aquí: el shader va embebido y se especializa para este tamaño.
const WIDTH:  u32 = 1080u;
const HEIGHT: u32 = 1920u;

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

// `src` se enlaza con un view de formato no-sRGB (Rgba8Unorm) sobre la textura sRGB, así
// que textureLoad devuelve los bytes gamma del framebuffer tal cual — los mismos que
// swscale recibía— sin la costosa conversión sRGB↔linear (un pow por canal por píxel).
fn rgb_gamma(p: vec2<i32>) -> vec3<f32> {
    return textureLoad(src, p, 0).rgb;
}

// BT.601 limited range (lo que produce swscale para yuv420p por defecto).
fn luma(c: vec3<f32>) -> f32 {
    let y = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b; // 0..1 full-range luma
    return 16.0 + 219.0 * y;                          // → 16..235
}
fn chroma_u(c: vec3<f32>) -> f32 {
    let y = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b;
    return 128.0 + 112.0 * (c.b - y) / 0.886;
}
fn chroma_v(c: vec3<f32>) -> f32 {
    let y = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b;
    return 128.0 + 112.0 * (c.r - y) / 0.701;
}

fn byte(v: f32) -> u32 {
    return u32(clamp(v + 0.5, 0.0, 255.0));
}

// ── Plano Y: cada invocación empaqueta 4 píxeles Y horizontales en un u32 ──
@compute @workgroup_size(8, 8, 1)
fn to_y(@builtin(global_invocation_id) id: vec3<u32>) {
    let qx = id.x; // columna de "quad" horizontal (4 px)
    let y  = id.y;
    if (qx * 4u >= WIDTH || y >= HEIGHT) { return; }

    let bx = i32(qx * 4u);
    let row = i32(y);
    var word: u32 = 0u;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let yv = byte(luma(rgb_gamma(vec2<i32>(bx + i32(i), row))));
        word = word | (yv << (8u * i));
    }
    out[y * (WIDTH / 4u) + qx] = word;
}

// ── Planos U y V: subsampleo 4:2:0. Cada invocación produce 4 muestras croma
//    horizontales (un u32 de U y otro de V), promediando bloques 2x2 ──
@compute @workgroup_size(8, 8, 1)
fn to_uv(@builtin(global_invocation_id) id: vec3<u32>) {
    let qx = id.x; // "quad" de 4 muestras croma = 8 px fuente de ancho
    let cy = id.y; // fila croma
    let cw = WIDTH / 2u;
    let ch = HEIGHT / 2u;
    if (qx * 4u >= cw || cy >= ch) { return; }

    let y_words = WIDTH * HEIGHT / 4u; // tamaño del plano Y en u32
    let u_words = cw * ch / 4u;

    var wu: u32 = 0u;
    var wv: u32 = 0u;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let sx = i32((qx * 4u + i) * 2u);
        let sy = i32(cy * 2u);
        let c00 = rgb_gamma(vec2<i32>(sx,     sy));
        let c10 = rgb_gamma(vec2<i32>(sx + 1, sy));
        let c01 = rgb_gamma(vec2<i32>(sx,     sy + 1));
        let c11 = rgb_gamma(vec2<i32>(sx + 1, sy + 1));
        let avg = (c00 + c10 + c01 + c11) * 0.25;
        wu = wu | (byte(chroma_u(avg)) << (8u * i));
        wv = wv | (byte(chroma_v(avg)) << (8u * i));
    }
    let idx = cy * (cw / 4u) + qx;
    out[y_words + idx]            = wu;
    out[y_words + u_words + idx]  = wv;
}
