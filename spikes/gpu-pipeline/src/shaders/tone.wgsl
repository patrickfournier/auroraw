// Camera RGB -> working space, exposure, tone curve, working -> display, 3D LUT (standing in
// for an ICC display profile), sRGB encoding, 8-bit packing.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> inter: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> lut: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> out: array<u32>;

fn aces(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn srgb_oetf(l: vec3<f32>) -> vec3<f32> {
    let lo = l * 12.92;
    let hi = 1.055 * pow(max(l, vec3<f32>(0.0031308)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, l <= vec3<f32>(0.0031308));
}

fn lut_at(i: u32, j: u32, k: u32) -> vec3<f32> {
    return lut[i + p.lut_size * (j + p.lut_size * k)].xyz;
}

fn lut_apply(c: vec3<f32>) -> vec3<f32> {
    let m = f32(p.lut_size - 1u);
    let pos = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * m;
    let base = min(floor(pos), vec3<f32>(m - 1.0));
    let f = pos - base;
    let i = u32(base.x); let j = u32(base.y); let k = u32(base.z);
    let c00 = mix(lut_at(i, j, k), lut_at(i + 1u, j, k), f.x);
    let c10 = mix(lut_at(i, j + 1u, k), lut_at(i + 1u, j + 1u, k), f.x);
    let c01 = mix(lut_at(i, j, k + 1u), lut_at(i + 1u, j, k + 1u), f.x);
    let c11 = mix(lut_at(i, j + 1u, k + 1u), lut_at(i + 1u, j + 1u, k + 1u), f.x);
    return mix(mix(c00, c10, f.y), mix(c01, c11, f.y), f.z);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let cam = inter[(gid.y + p.in_y0) * p.in_stride + gid.x + p.in_x0].xyz;
    var w = vec3<f32>(dot(p.cam0.xyz, cam), dot(p.cam1.xyz, cam), dot(p.cam2.xyz, cam));
    w = aces(max(w, vec3<f32>(0.0)) * p.exposure);
    let d = vec3<f32>(dot(p.disp0.xyz, w), dot(p.disp1.xyz, w), dot(p.disp2.xyz, w));
    let enc = lut_apply(srgb_oetf(clamp(d, vec3<f32>(0.0), vec3<f32>(1.0))));
    let q = vec3<u32>(clamp(enc, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5);
    out[(gid.y + p.out_y0) * p.out_stride + gid.x + p.out_x0] = q.x | (q.y << 8u) | (q.z << 16u) | (255u << 24u);
}
