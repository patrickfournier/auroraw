//! An operation plugin with a GPU shader and a CPU twin. `describe` returns its declaration; the
//! shader is data, and the CPU version runs in the sandbox, so both must agree.
use kernels::saturation;

const DECLARATION: &str = r#"{
  "id": "org.auroraw.example.saturation",
  "version": "1.0.0",
  "api": 1,
  "family": "operation",
  "name": "Saturation",
  "panel": "Colour",
  "stage": "scene-linear",
  "after": ["demosaic"],
  "before": ["tone"],
  "params": [{ "name": "amount", "type": "float", "min": 0.0, "max": 3.0, "default": 1.5 }],
  "permissions": [],
  "gpu": { "contract": 1, "entry": "main", "wgsl": "@group(0) @binding(0) var<uniform> p: Params;\n@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;\n@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;\n@compute @workgroup_size(16, 16)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n  if (gid.x >= p.tile_w || gid.y >= p.tile_h) { return; }\n  let i = gid.y * p.tile_w + gid.x;\n  let c = src[i];\n  let l = dot(c.xyz, vec3<f32>(0.2126, 0.7152, 0.0722));\n  let amount = p.op0.x;\n  dst[i] = vec4<f32>(max(vec3<f32>(l) + (c.xyz - vec3<f32>(l)) * amount, vec3<f32>(0.0)), c.w);\n}\n" },
  "cpu": { "export": "process" }
}"#;

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// The declaration: address in the high half, length in the low half.
#[unsafe(no_mangle)]
pub extern "C" fn describe() -> u64 {
    ((DECLARATION.as_ptr() as u64) << 32) | DECLARATION.len() as u64
}

/// The CPU twin of the shader, on `w` by `h` RGBA f32 pixels, with the plugin's one parameter.
#[unsafe(no_mangle)]
pub extern "C" fn process(px: *mut f32, w: u32, h: u32, amount: f32) {
    saturation(unsafe { std::slice::from_raw_parts_mut(px, (w * h * 4) as usize) }, amount);
}
