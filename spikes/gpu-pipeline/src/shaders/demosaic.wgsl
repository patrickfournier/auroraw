// Gradient-corrected bilinear demosaicing (Malvar, He, Cutler) of an RGGB mosaic, with black
// level subtraction and white balance. Reads the whole mosaic, writes one tile of camera RGB.
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mosaic: array<u32>; // two 16-bit samples per word
@group(0) @binding(2) var<storage, read_write> inter: array<vec4<f32>>;

fn mirror(i: i32, n: i32) -> i32 {
    var r = i;
    if (r < 0) { r = -r; }
    if (r >= n) { r = 2 * n - 2 - r; }
    return clamp(r, 0, n - 1);
}

fn raw(x: i32, y: i32) -> f32 {
    let ix = mirror(x, i32(p.width));
    let iy = mirror(y, i32(p.height));
    let idx = u32(iy) * p.width + u32(ix);
    let word = mosaic[idx >> 1u];
    let v = (word >> ((idx & 1u) * 16u)) & 0xffffu;
    return (f32(v) - p.black) / (p.white - p.black);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }
    let x = i32(p.tile_x + gid.x);
    let y = i32(p.tile_y + gid.y);
    let c = raw(x, y);
    let n = raw(x, y - 1); let s = raw(x, y + 1);
    let w = raw(x - 1, y); let e = raw(x + 1, y);
    let nn = raw(x, y - 2); let ss = raw(x, y + 2);
    let ww = raw(x - 2, y); let ee = raw(x + 2, y);
    let nw = raw(x - 1, y - 1); let ne = raw(x + 1, y - 1);
    let sw = raw(x - 1, y + 1); let se = raw(x + 1, y + 1);

    let px = (u32(x) ^ p.cfa_flip) & 1u;
    let py = (u32(y) ^ (p.cfa_flip >> 1u)) & 1u;
    var rgb = vec3<f32>(0.0);
    // Green interpolated at a red or blue site.
    let g_at_rb = (4.0 * c + 2.0 * (n + s + e + w) - (nn + ss + ee + ww)) / 8.0;
    if (px == 0u && py == 0u) {          // red site
        let b_here = (6.0 * c + 2.0 * (nw + ne + sw + se) - 1.5 * (nn + ss + ee + ww)) / 8.0;
        rgb = vec3<f32>(c, g_at_rb, b_here);
    } else if (px == 1u && py == 1u) {   // blue site
        let r_here = (6.0 * c + 2.0 * (nw + ne + sw + se) - 1.5 * (nn + ss + ee + ww)) / 8.0;
        rgb = vec3<f32>(r_here, g_at_rb, c);
    } else if (px == 1u && py == 0u) {   // green site in a red row
        let r_here = (5.0 * c + 4.0 * (w + e) - (nw + ne + sw + se) - (ww + ee) + 0.5 * (nn + ss)) / 8.0;
        let b_here = (5.0 * c + 4.0 * (n + s) - (nw + ne + sw + se) - (nn + ss) + 0.5 * (ww + ee)) / 8.0;
        rgb = vec3<f32>(r_here, c, b_here);
    } else {                             // green site in a blue row
        let r_here = (5.0 * c + 4.0 * (n + s) - (nw + ne + sw + se) - (nn + ss) + 0.5 * (ww + ee)) / 8.0;
        let b_here = (5.0 * c + 4.0 * (w + e) - (nw + ne + sw + se) - (ww + ee) + 0.5 * (nn + ss)) / 8.0;
        rgb = vec3<f32>(r_here, c, b_here);
    }
    rgb = max(rgb, vec3<f32>(0.0)) * p.wb.xyz;
    inter[gid.y * p.tile_w + gid.x] = vec4<f32>(rgb, 1.0);
}
