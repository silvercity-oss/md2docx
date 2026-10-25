use super::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WebSettings {
    pub divs: Vec<Div>,
}

impl WebSettings {
    pub fn new() -> WebSettings {
        Default::default()
    }
}
