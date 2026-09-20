// Unsharp mask, local contrast and a radial exposure mask, in one pass.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> base: array<vec4<f32>>;   // denoised
@group(0) @binding(2) var<storage, read> small: array<vec4<f32>>;   // blurred, small radius
@group(0) @binding(3) var<storage, read> large: array<vec4<f32>>;   // blurred, large radius
@group(0) @binding(4) var<storage, read_write> dst: array<vec4<f32>>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let i = gid.y * p.tile_w + gid.x;
    let b = base[i].xyz;
    var v = b + p.op1.x * (b - small[i].xyz) + p.op2.x * (b - large[i].xyz);
    let uv = vec2<f32>(f32(gid.x) / f32(p.tile_w), f32(gid.y) / f32(p.tile_h));
    let m = 1.0 - smoothstep(0.5, 1.0, distance(uv, p.op3.xy) / max(p.op3.z, 1e-4));
    v = max(v, vec3<f32>(0.0)) * exp2(p.op3.w * m);
    dst[i] = vec4<f32>(v, 1.0);
}
