//! The LaTeX → OMML converter on its own: it must never panic or loop, whatever the formula.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(latex) = std::str::from_utf8(data) else { return };
    let _ = md2docx::__internals::latex_to_omml_checked(latex);
});
