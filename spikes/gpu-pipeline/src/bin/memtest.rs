//! How many 64 MB storage buffers can this adapter allocate before running out of memory?
use gpu_pipeline::gpu::Gpu;
fn main() {
    let adapter = std::env::args().nth(1).unwrap_or_else(|| "vulkan nvidia".into());
    let g = Gpu::select(&adapter).unwrap();
    let mut keep = Vec::new();
    for i in 1..=200 {
        let scope = g.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let b = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 64 << 20, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let err = pollster::block_on(scope.pop());
        if err.is_some() {
            println!("out of memory after {} buffers of 64 MB = {} MB", i - 1, (i - 1) * 64);
            return;
        }
        keep.push(b);
    }
    println!("allocated 200 buffers (12.8 GB) without failing");
}
