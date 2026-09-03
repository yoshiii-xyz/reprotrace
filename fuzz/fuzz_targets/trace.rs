#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = reprotrace::parse_sccache_log_bytes(data);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<reprotrace::Trace>(text);
    }
});
