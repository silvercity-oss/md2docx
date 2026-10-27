//! Raw HTML, tables and footnotes through `analyze`, which runs the full parse without packaging.
#![no_main]

use libfuzzer_sys::fuzz_target;
use md2docx::{analyze, Config};

fuzz_target!(|data: &[u8]| {
    let Ok(markdown) = std::str::from_utf8(data) else { return };
    let mut config = Config::untrusted();
    config.limits.max_input_bytes = 64 * 1024;
    config.limits.max_elements = 5_000;
    // Wrapping the input makes the HTML, table and footnote paths reachable from any byte string.
    let wrapped = format!("<div>\n\n{markdown}\n\n</div>\n\n| a | b |\n|---|---|\n| {markdown} | x |\n\n[^1]: {markdown}\n");
    let _ = analyze(&wrapped, &config);
});
