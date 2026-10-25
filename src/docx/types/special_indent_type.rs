
// INFO: wasm-bindgen only allow c-style enum for now
//       Please convert typescript type to following type.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SpecialIndentType {
    FirstLine(i32),
    Hanging(i32),
}

#[derive(Copy, Clone, Debug)]
pub enum SpecialIndentKind {
    FirstLine,
    Hanging,
}

