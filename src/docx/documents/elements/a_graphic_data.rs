use super::*;
use std::io::Write;
use std::str::FromStr;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

/*
  20.1.2.2.17
  graphicData (Graphic Object Data)
  This element specifies the reference to a graphic object within the document. This graphic object is provided
  entirely by the document authors who choose to persist this data within the document.
*/
#[derive(Debug, Clone, PartialEq)]
pub struct AGraphicData {
    pub data_type: GraphicDataType,
    pub children: Vec<GraphicDataChild>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicDataChild {
    Shape(WpsShape),
    Pic(Pic),
}

impl GraphicDataType {
    fn to_uri(&self) -> &str {
        match *self {
            GraphicDataType::Picture => "http://schemas.openxmlformats.org/drawingml/2006/picture",
            GraphicDataType::WpShape => {
                "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
            }
            _ => "",
        }
    }
}

impl FromStr for GraphicDataType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.ends_with("picture") {
            return Ok(GraphicDataType::Picture);
        }
        if s.ends_with("wordprocessingShape") {
            return Ok(GraphicDataType::WpShape);
        }
        Ok(GraphicDataType::Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicDataType {
    Picture,
    WpShape,
    Unsupported,
}

impl AGraphicData {
    pub fn new(data_type: GraphicDataType) -> AGraphicData {
        AGraphicData {
            data_type,
            children: vec![],
        }
    }

    pub fn add_shape(mut self, shape: WpsShape) -> Self {
        self.children.push(GraphicDataChild::Shape(shape));
        self
    }
}

impl BuildXML for AGraphicData {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_graphic_data(self.data_type.to_uri())?
            .apply_each(&self.children, |ch, b| match ch {
                GraphicDataChild::Shape(t) => b.add_child(t),
                GraphicDataChild::Pic(t) => b.add_child(t),
            })?
            .close()?
            .into_inner()
    }
}
