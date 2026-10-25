use crate::docx::documents::{BuildXML, Level};
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct AbstractNumbering {
    pub id: usize,
    pub style_link: Option<String>,
    pub num_style_link: Option<String>,
    pub levels: Vec<Level>,
    pub multi_level_type: Option<String>,
}

impl AbstractNumbering {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            style_link: None,
            num_style_link: None,
            levels: vec![],
            multi_level_type: None,
        }
    }

    pub fn add_level(mut self, level: Level) -> Self {
        self.levels.push(level);
        self
    }

    pub fn num_style_link(mut self, link: impl Into<String>) -> Self {
        self.num_style_link = Some(link.into());
        self
    }

    pub fn style_link(mut self, link: impl Into<String>) -> Self {
        self.style_link = Some(link.into());
        self
    }
}

impl BuildXML for AbstractNumbering {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let mut builder = XMLBuilder::from(stream).open_abstract_num(&self.id.to_string())?;

        // 添加 multiLevelType 元素（如果存在）
        if let Some(ref multi_level_type) = self.multi_level_type {
            builder = builder.multi_level_type(multi_level_type)?;
        }

        builder.add_children(&self.levels)?.close()?.into_inner()
    }
}

