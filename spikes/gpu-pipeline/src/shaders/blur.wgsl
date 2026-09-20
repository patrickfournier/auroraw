// One direction of a box blur of radius `blur.x`; `blur.y` selects horizontal (0) or vertical (1).
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let r = i32(p.blur.x);
    var acc = vec3<f32>(0.0);
    for (var i = -r; i <= r; i = i + 1) {
        var x = i32(gid.x);
        var y = i32(gid.y);
        if (p.blur.y == 0u) { x = clamp(x + i, 0, i32(p.tile_w) - 1); } else { y = clamp(y + i, 0, i32(p.tile_h) - 1); }
        acc = acc + src[u32(y) * p.tile_w + u32(x)].xyz;
    }
    dst[gid.y * p.tile_w + gid.x] = vec4<f32>(acc / f32(2 * r + 1), 1.0);
}
