//! Spike 4: a plugin host on wasmtime. Throwaway code that measures whether WebAssembly is a
//! workable sandbox for image operations and importers (docs/technical-spikes.md, §5).

pub use wasmtime::Result;
use std::path::PathBuf;
use std::time::Instant;
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::{WasiP1Ctx, add_to_linker_sync};

pub fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

pub fn pct(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() as f64 * q) as usize).min(s.len() - 1)]
}

pub fn plugin_path(rel: &str) -> PathBuf {
    let base = std::env::var("PLUGINS").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("plugins/target/wasm32-wasip1/release"));
    base.join(rel)
}

/// What the host holds for one running plugin.
pub struct Ctx {
    pub wasi: WasiP1Ctx,
    pub limits: StoreLimits,
}

/// A plugin instance with its own memory and its own limits.
pub struct Plugin {
    pub store: Store<Ctx>,
    pub instance: Instance,
    pub memory: Memory,
    alloc: TypedFunc<u32, u32>,
}

/// What the host allows a plugin to do. Nothing, unless granted.
#[derive(Clone, Default)]
pub struct Grants {
    /// Folders the plugin may read (read-only), by host path.
    pub read_dirs: Vec<PathBuf>,
    pub memory_limit: Option<usize>,
}

pub fn engine(epoch: bool, fuel: bool) -> Result<Engine> {
    let mut c = wasmtime::Config::new();
    c.epoch_interruption(epoch);
    c.consume_fuel(fuel);
    Engine::new(&c)
}

pub fn load(engine: &Engine, path: &std::path::Path) -> Result<Module> {
    Module::from_file(engine, path)
}

pub fn instantiate(engine: &Engine, module: &Module, grants: &Grants) -> Result<Plugin> {
    let mut wasi = WasiCtxBuilder::new();
    // No environment, no arguments, no standard streams, no clock beyond what WASI needs,
    // no network, and no folders unless granted.
    for d in &grants.read_dirs {
        wasi.preopened_dir(d, "/granted", wasmtime_wasi::FsPerms::ReadOnly)?;
    }
    let limits = StoreLimitsBuilder::new().memory_size(grants.memory_limit.unwrap_or(1 << 30)).trap_on_grow_failure(true).build();
    let mut store = Store::new(engine, Ctx { wasi: wasi.build_p1(), limits });
    store.limiter(|c| &mut c.limits);
    // A new store has no fuel and an epoch deadline of zero. Give it a large budget before anything
    // runs, so that instantiating never fails for want of fuel; callers then set their own limits.
    // (Both calls are harmless when the engine was not configured for them.)
    let _ = store.set_fuel(u64::MAX / 2);
    store.set_epoch_deadline(u64::MAX / 2);
    let mut linker: Linker<Ctx> = Linker::new(engine);
    add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)?;
    let instance = linker.instantiate(&mut store, module)?;
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| wasmtime::format_err!("no memory export"))?;
    let alloc = instance.get_typed_func::<u32, u32>(&mut store, "alloc")?;
    Ok(Plugin { store, instance, memory, alloc })
}

impl Plugin {
    /// Reserves `n` bytes in the plugin's memory and returns their address there.
    pub fn alloc(&mut self, n: usize) -> Result<u32> {
        Ok(self.alloc.call(&mut self.store, n as u32)?)
    }

    pub fn write(&mut self, at: u32, bytes: &[u8]) -> Result<()> {
        self.memory.write(&mut self.store, at as usize, bytes)?;
        Ok(())
    }

    pub fn read(&mut self, at: u32, out: &mut [u8]) -> Result<()> {
        self.memory.read(&self.store, at as usize, out)?;
        Ok(())
    }

    pub fn func<P: wasmtime::WasmParams, R: wasmtime::WasmResults>(&mut self, name: &str) -> Result<TypedFunc<P, R>> {
        Ok(self.instance.get_typed_func::<P, R>(&mut self.store, name)?)
    }
}

pub fn f32_bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

pub fn f32_bytes_mut(v: &mut [f32]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, std::mem::size_of_val(v)) }
}
