//! Lists the adapters wgpu can see, with the limits that matter for a tiled image pipeline.

fn main() {
    let instance = wgpu::Instance::default();
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    println!("{} adapter(s)", adapters.len());
    for adapter in adapters {
        let info = adapter.get_info();
        let limits = adapter.limits();
        println!(
            "\n{} [{:?}, {:?}]\n  driver: {} {}\n  max buffer: {} MiB, max storage binding: {} MiB\n  max 2D texture: {}, workgroup storage: {} KiB, invocations/workgroup: {}\n  f16 in shaders: {}",
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info,
            limits.max_buffer_size >> 20,
            limits.max_storage_buffer_binding_size >> 20,
            limits.max_texture_dimension_2d,
            limits.max_compute_workgroup_storage_size >> 10,
            limits.max_compute_invocations_per_workgroup,
            adapter.features().contains(wgpu::Features::SHADER_F16),
        );
    }
}
