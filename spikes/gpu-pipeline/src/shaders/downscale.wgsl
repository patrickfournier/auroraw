// Box downscale of the cached camera RGB, for the fit-to-screen view.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    var acc = vec3<f32>(0.0);
    var count = 0.0;
    for (var j = 0u; j < p.factor; j = j + 1u) {
        for (var i = 0u; i < p.factor; i = i + 1u) {
            let sx = gid.x * p.factor + i;
            let sy = gid.y * p.factor + j;
            if (sx < p.src_w && sy < p.src_h) {
                acc = acc + src[sy * p.src_w + sx].xyz;
                count = count + 1.0;
            }
        }
    }
    dst[(gid.y + p.out_y0) * p.tile_w + gid.x] = vec4<f32>(acc / max(count, 1.0), 1.0);
}
