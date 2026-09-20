// Generic demosaicing for any colour filter pattern with a 6x6 period (X-Trans, or a 2x2 Bayer
// tiled up), by inverse-distance weighted averaging of the same-colour samples in a 7x7 window.
// Simple on purpose: it shows that the demosaicing stage can be swapped, not that it is good.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mosaic: array<u32>;
@group(0) @binding(2) var<storage, read_write> inter: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> cfa: array<u32>; // 36 colour indices: 0 red, 1 green, 2 blue

fn mirror(i: i32, n: i32) -> i32 {
    var r = i;
    if (r < 0) { r = -r; }
    if (r >= n) { r = 2 * n - 2 - r; }
    return clamp(r, 0, n - 1);
}

fn raw(ix: i32, iy: i32) -> f32 {
    let idx = u32(iy) * p.width + u32(ix);
    let word = mosaic[idx >> 1u];
    let v = (word >> ((idx & 1u) * 16u)) & 0xffffu;
    return (f32(v) - p.black) / (p.white - p.black);
}

fn colour_at(ix: i32, iy: i32) -> u32 {
    return cfa[u32(iy % 6) * 6u + u32(ix % 6)];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let x = i32(p.tile_x + gid.x);
    let y = i32(p.tile_y + gid.y);
    // No dynamic indexing of vector components: DirectX's FXC compiler rejects it in a loop.
    var sum = vec3<f32>(0.0);
    var weight = vec3<f32>(0.0);
    for (var dy = -3; dy <= 3; dy = dy + 1) {
        for (var dx = -3; dx <= 3; dx = dx + 1) {
            let mx = mirror(x + dx, i32(p.width));
            let my = mirror(y + dy, i32(p.height));
            let c = colour_at(mx, my);
            let mask = vec3<f32>(f32(c == 0u), f32(c == 1u), f32(c == 2u));
            let w = 1.0 / (1.0 + f32(dx * dx + dy * dy));
            sum = sum + mask * (w * raw(mx, my));
            weight = weight + mask * w;
        }
    }
    var rgb = sum / max(weight, vec3<f32>(1e-6));
    let own = colour_at(x, y);
    let own_mask = vec3<f32>(f32(own == 0u), f32(own == 1u), f32(own == 2u));
    rgb = mix(rgb, vec3<f32>(raw(x, y)), own_mask);
    rgb = max(rgb, vec3<f32>(0.0)) * p.wb.xyz;
    inter[gid.y * p.tile_w + gid.x] = vec4<f32>(rgb, 1.0);
}
