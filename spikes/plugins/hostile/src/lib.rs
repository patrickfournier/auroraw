//! A plugin that misbehaves on purpose, to see what the host does about it. Every function is
//! something a broken or malicious plugin could try.
use std::io::ErrorKind;

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

fn text(ptr: *const u8, len: u32) -> String {
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).into_owned()
}

/// A code for an error: the OS error number if there is one, else 1000 plus the kind.
fn code(e: std::io::Error) -> i32 {
    match e.raw_os_error() {
        Some(n) => -n,
        None => -1000 - match e.kind() { ErrorKind::Unsupported => 1, ErrorKind::NotFound => 2, ErrorKind::PermissionDenied => 3, _ => 9 },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_panic() -> u32 {
    panic!("this plugin panics on purpose")
}

#[unsafe(no_mangle)]
pub extern "C" fn do_loop() -> u32 {
    let mut i = 0u64;
    loop {
        i = std::hint::black_box(i.wrapping_add(1));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_alloc_bomb(mb: u32) -> u32 {
    let mut keep: Vec<Vec<u8>> = Vec::new();
    for _ in 0..mb {
        let mut v = vec![0u8; 1 << 20];
        v[0] = 1;
        keep.push(v);
    }
    keep.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn do_stack_overflow(n: u32) -> u32 {
    let pad = [n as u8; 256];
    if n == u32::MAX { return pad[3] as u32; }
    do_stack_overflow(n + 1) + std::hint::black_box(pad[7] as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn do_read_file(p: *const u8, len: u32) -> i32 {
    match std::fs::read(text(p, len)) { Ok(b) => b.len() as i32, Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_list_dir(p: *const u8, len: u32) -> i32 {
    match std::fs::read_dir(text(p, len)) { Ok(d) => d.count() as i32, Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_write_file(p: *const u8, len: u32) -> i32 {
    match std::fs::write(text(p, len), b"written by a plugin") { Ok(()) => 0, Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_connect() -> i32 {
    match std::net::TcpStream::connect("93.184.216.34:80") { Ok(_) => 0, Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_spawn_process() -> i32 {
    match std::process::Command::new("ls").output() { Ok(_) => 0, Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_spawn_thread() -> i32 {
    match std::thread::Builder::new().spawn(|| 1) { Ok(h) => h.join().map(|v| v).unwrap_or(-2), Err(e) => code(e) }
}

#[unsafe(no_mangle)]
pub extern "C" fn do_env_count() -> i32 {
    std::env::vars().count() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn do_args_count() -> i32 {
    std::env::args().count() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn do_clock_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(-1)
}
