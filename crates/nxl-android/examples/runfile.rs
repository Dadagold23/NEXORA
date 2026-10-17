fn main() {
    let dir = std::env::args().nth(1).unwrap_or_default();
    let name = dir.rsplit(|c| c == '/' || c == '\\').next().unwrap().to_string();
    let text = std::fs::read_to_string(&dir).unwrap();
    match nxl_android::run_source(&name, &text) {
        Ok(out) => println!("OK\n{}", out),
        Err(e) => println!("ERR {}", e),
    }
}