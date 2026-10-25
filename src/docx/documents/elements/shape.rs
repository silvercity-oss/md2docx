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

impl Shape {}

// impl BuildXML for Shape {
//     fn build(&self) -> Vec<u8> {
//         let b = XMLBuilder::new();
//         let mut b = b.open_pict();
//         b = b.add_child(t),
//         b.close().build()
//     }
// }
