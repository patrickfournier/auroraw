// Non-local means denoising on camera-space RGB: each pixel becomes a weighted average of the
// pixels in a (2R+1)^2 search window, weighted by how much their (2P+1)^2 patches resemble its own.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;

fn at(x: i32, y: i32) -> vec3<f32> {
    let cx = clamp(x, 0, i32(p.tile_w) - 1);
    let cy = clamp(y, 0, i32(p.tile_h) - 1);
    return src[u32(cy) * p.tile_w + u32(cx)].xyz;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);
    let r = i32(p.op0.z);
    let pr = i32(p.op0.w);
    let h2 = max(p.op0.x * p.op0.x, 1e-8);
    let norm = 1.0 / f32((2 * pr + 1) * (2 * pr + 1) * 3);
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var sy = -r; sy <= r; sy = sy + 1) {
        for (var sx = -r; sx <= r; sx = sx + 1) {
            var d = 0.0;
            for (var py = -pr; py <= pr; py = py + 1) {
                for (var px = -pr; px <= pr; px = px + 1) {
                    let diff = at(x + px, y + py) - at(x + sx + px, y + sy + py);
                    d = d + dot(diff, diff);
                }
            }
            let w = exp(-(d * norm) / h2);
            acc = acc + w * at(x + sx, y + sy);
            wsum = wsum + w;
        }
    }
    dst[gid.y * p.tile_w + gid.x] = vec4<f32>(acc / wsum, 1.0);
}
