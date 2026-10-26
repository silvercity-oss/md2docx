use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::types::*;
use crate::docx::{create_hyperlink_rid, generate_hyperlink_id, xml_builder::*};

#[derive(Debug, Clone, PartialEq)]
pub enum HyperlinkData {
    External {
        rid: String,
        // path is writer only
        path: String,
        /// A bookmark in the target document to land on.
        anchor: Option<String>,
    },
    Anchor {
        anchor: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hyperlink {
    pub link: HyperlinkData,
    pub history: Option<usize>,
    pub children: Vec<ParagraphChild>,
}

impl Hyperlink {
    pub fn new(value: impl Into<String>, t: HyperlinkType) -> Self {
        let link = {
            match t {
                HyperlinkType::External => HyperlinkData::External {
                    rid: create_hyperlink_rid(generate_hyperlink_id()),
                    path: escape_owned(value.into()),
                    anchor: None,
                },
                HyperlinkType::Anchor => HyperlinkData::Anchor { anchor: value.into() },
            }
        };
        Hyperlink {
            link,
            history: None,
            children: vec![],
        }
    }

    /// A link to another file that also names a bookmark in it.
    pub fn external_with_anchor(value: impl Into<String>, anchor: impl Into<String>) -> Self {
        let mut h = Hyperlink::new(value, HyperlinkType::External);
        if let HyperlinkData::External { anchor: a, .. } = &mut h.link {
            *a = Some(anchor.into());
        }
        h
    }

    pub fn add_run(mut self, run: Run) -> Self {
        self.children.push(ParagraphChild::Run(Box::new(run)));
        self
    }
}

impl BuildXML for Hyperlink {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .apply(|b| match self.link {
                HyperlinkData::Anchor { ref anchor } => {
                    b.open_hyperlink(None, Some(anchor.clone()).as_ref(), Some(self.history.unwrap_or(1)))
                }
                HyperlinkData::External {
                    ref rid, ref anchor, ..
                } => b.open_hyperlink(
                    Some(rid.clone()).as_ref(),
                    anchor.as_ref(),
                    Some(self.history.unwrap_or(1)),
                ),
            })?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}
