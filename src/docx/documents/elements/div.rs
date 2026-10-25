#[derive(Debug, Clone, PartialEq)]
pub struct Div {
    pub id: String,
    pub margin_left: usize,
    pub margin_right: usize,
    pub margin_top: usize,
    pub margin_bottom: usize,
    pub divs_child: Vec<Div>,
}

impl Default for Div {
    fn default() -> Self {
        Self {
            id: "".to_string(),
            margin_left: 0,
            margin_right: 0,
            margin_top: 0,
            margin_bottom: 0,
            divs_child: vec![],
        }
    }
}

impl Div {}
