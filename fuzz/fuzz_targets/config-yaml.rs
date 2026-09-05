#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = serde_yaml::from_str::<inferqos_config::Config>(text).map(|config| config.validate());
    }
});
