//! Whole pipeline under the untrusted-input policy: any byte string must give `Ok` or an error,
//! never a panic, hang or runaway allocation. The limits are tightened so one input stays cheap.
#![no_main]

use libfuzzer_sys::fuzz_target;
use md2docx::{markdown_to_docx, Config};

fuzz_target!(|data: &[u8]| {
    let Ok(markdown) = std::str::from_utf8(data) else { return };
    let mut config = Config::untrusted();
    config.limits.max_input_bytes = 64 * 1024;
    config.limits.max_elements = 5_000;
    let _ = markdown_to_docx(markdown, &config);
});
