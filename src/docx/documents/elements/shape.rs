
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    pub style: Option<String>,
    pub image_data: Option<ImageData>,
}
// Experimental, For now reader only.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageData {
    pub id: String,
}

impl Shape {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn image_data(mut self, id: impl Into<String>) -> Self {
        self.image_data = Some(ImageData { id: id.into() });
        self
    }
}

// impl BuildXML for Shape {
//     fn build(&self) -> Vec<u8> {
//         let b = XMLBuilder::new();
//         let mut b = b.open_pict();
//         b = b.add_child(t),
//         b.close().build()
//     }
// }
