//! Prints what rawler reports for a RAW file.
fn main() {
    for path in std::env::args().skip(1) {
        match rawler::decode_file(&path) {
            Ok(r) => println!("{path}\n  {} {} | {}x{} cpp {} bps {} | photometric {:?}\n  crop {:?} active {:?}\n  white {:?} black {:?}", r.clean_make, r.clean_model, r.width, r.height, r.cpp, r.bps, r.photometric, r.crop_area, r.active_area, r.whitelevel, r.blacklevel.levels.iter().take(4).collect::<Vec<_>>()),
            Err(e) => println!("{path}: {e}"),
        }
    }
}
