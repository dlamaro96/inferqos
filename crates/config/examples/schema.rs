use inferqos_config::Config;
fn main() {
    let schema = Config::json_schema();
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, schema).expect("schema output must be writable");
    } else {
        println!("{schema}");
    }
}
