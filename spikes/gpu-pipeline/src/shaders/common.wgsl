// Parameters shared by every pass of the spike pipeline. Kept identical in Rust (`Params`).
struct Params {
    width: u32,       // full mosaic size
    height: u32,
    tile_x: u32,      // region of the image processed by this dispatch (image coordinates)
    tile_y: u32,
    tile_w: u32,
    tile_h: u32,
    black: f32,
    white: f32,
    wb: vec4<f32>,    // white balance multipliers r, g, b
    cam0: vec4<f32>,  // rows of the camera -> working space matrix
    cam1: vec4<f32>,
    cam2: vec4<f32>,
    disp0: vec4<f32>, // rows of the working space -> display (linear) matrix
    disp1: vec4<f32>,
    disp2: vec4<f32>,
    exposure: f32,
    lut_size: u32,
    in_x0: u32,       // tone pass: where the region starts in the intermediate buffer
    in_y0: u32,
    in_stride: u32,   // tone pass: row length of the intermediate buffer
    out_x0: u32,      // tone pass: where the region starts in the output buffer
    out_y0: u32,
    out_stride: u32,
    src_w: u32,       // downscale pass: source size and box factor
    src_h: u32,
    factor: u32,
    cfa_flip: u32,    // demosaic: bit 0 flips the column parity, bit 1 the row parity
    op0: vec4<f32>,   // denoise: h, unused, search radius, patch radius
    op1: vec4<f32>,   // sharpen: amount, blur radius
    op2: vec4<f32>,   // local contrast: gain, blur radius
    op3: vec4<f32>,   // mask: centre x, centre y, radius, exposure change (EV)
    blur: vec4<u32>,  // blur pass: radius, direction (0 horizontal, 1 vertical)
};
