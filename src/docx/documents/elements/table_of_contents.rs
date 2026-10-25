use std::io::Write;

use crate::docx::types::*;
use crate::docx::xml_builder::*;
use crate::docx::{documents::*, escape};

#[derive(Debug, Clone, PartialEq)]
pub enum TocContent {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableOfContentsReviewData {
    pub author: String,
    pub date: String,
}

/// <https://c-rex.net/projects/samples/ooxml/e1/Part4/OOXML_P4_DOCX_TOCTOC_topic_ID0ELZO1.html>
/// This struct is only used by writers
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableOfContents {
    pub instr: InstrToC,
    pub items: Vec<TableOfContentsItem>,
    // don't use
    pub auto: bool,
    pub dirty: bool,
    /// Skip StructuredDataTag rendering
    pub without_sdt: bool,
    pub alias: Option<String>,
    pub page_ref_placeholder: Option<String>,
    // it is inserted in before toc.
    pub before_contents: Vec<TocContent>,
    // it is inserted in after toc.
    pub after_contents: Vec<TocContent>,
    pub delete: Option<TableOfContentsReviewData>,
    pub paragraph_property: Option<ParagraphProperty>,
}

impl TableOfContents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instr_text(s: &str) -> Self {
        let instr = InstrToC::with_instr_text(s);
        Self {
            instr,
            ..Self::default()
        }
    }

    pub fn heading_styles_range(mut self, start: usize, end: usize) -> Self {
        self.instr = self.instr.heading_styles_range(start, end);
        self
    }

    pub fn tc_field_identifier(mut self, f: Option<String>) -> Self {
        self.instr = self.instr.tc_field_identifier(f);
        self
    }

    pub fn add_style_with_level(mut self, s: StyleWithLevel) -> Self {
        self.instr = self.instr.add_style_with_level(s);
        self
    }

    pub fn hyperlink(mut self) -> Self {
        self.instr = self.instr.hyperlink();
        self
    }

    pub fn alias(mut self, a: impl Into<String>) -> Self {
        self.alias = Some(a.into());
        self
    }

    pub fn delete(mut self, author: impl Into<String>, date: impl Into<String>) -> Self {
        self.delete = Some(TableOfContentsReviewData {
            author: escape::escape_owned(author.into()),
            date: date.into(),
        });
        self
    }

    // pub fn tc_field_level_range(mut self, start: usize, end: usize) -> Self {
    //     self.instr = self.instr.tc_field_level_range(start, end);
    //     self
    // }

    pub fn add_item(mut self, t: TableOfContentsItem) -> Self {
        self.items.push(t);
        self
    }

    pub fn auto(mut self) -> Self {
        self.auto = true;
        self
    }

    pub fn dirty(mut self) -> Self {
        self.dirty = true;
        self
    }

    pub fn add_before_paragraph(mut self, p: Paragraph) -> Self {
        self.before_contents
            .push(TocContent::Paragraph(Box::new(p)));
        self
    }

    pub fn add_after_paragraph(mut self, p: Paragraph) -> Self {
        self.after_contents.push(TocContent::Paragraph(Box::new(p)));
        self
    }

    pub fn add_before_table(mut self, t: Table) -> Self {
        self.before_contents.push(TocContent::Table(Box::new(t)));
        self
    }

    pub fn add_after_table(mut self, t: Table) -> Self {
        self.after_contents.push(TocContent::Table(Box::new(t)));
        self
    }

    pub fn without_sdt(mut self) -> Self {
        self.without_sdt = true;
        self
    }

    pub fn paragraph_property(mut self, p: ParagraphProperty) -> Self {
        self.paragraph_property = Some(p);
        self
    }
}

impl BuildXML for TableOfContents {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let mut p = StructuredDataTagProperty::new();
        if let Some(ref alias) = self.alias {
            p = p.alias(alias);
        }

        if self.items.is_empty() {
            let mut b = XMLBuilder::from(stream);

            if !self.without_sdt {
                b = b
                    .open_structured_tag()?
                    .add_child(&p)?
                    .open_structured_tag_content()?;
            }

            for c in self.before_contents.iter() {
                match c {
                    TocContent::Paragraph(p) => {
                        b = b.add_child(&p)?;
                    }
                    TocContent::Table(t) => {
                        b = b.add_child(&t)?;
                    }
                }
            }

            let mut p1 = if let Some(ref del) = self.delete {
                Paragraph::new().add_delete(
                    Delete::new().author(&del.author).date(&del.date).add_run(
                        Run::new()
                            .add_field_char(FieldCharType::Begin, true)
                            .add_delete_instr_text(DeleteInstrText::TOC(self.instr.clone()))
                            .add_field_char(FieldCharType::Separate, false),
                    ),
                )
            } else {
                Paragraph::new().add_run(
                    Run::new()
                        .add_field_char(FieldCharType::Begin, true)
                        .add_instr_text(InstrText::TOC(self.instr.clone()))
                        .add_field_char(FieldCharType::Separate, false),
                )
            };

            if let Some(ref p) = self.paragraph_property {
                p1 = p1.paragraph_property(p.clone());
            }

            b = b.add_child(&p1)?;

            let p2 = Paragraph::new().add_run(Run::new().add_field_char(FieldCharType::End, false));

            if self.after_contents.is_empty() {
                b = b.add_child(&p2)?;
            } else {
                for (i, c) in self.after_contents.iter().enumerate() {
                    match c {
                        TocContent::Paragraph(p) => {
                            // Merge paragraph
                            if i == 0 {
                                let mut new_p = p.clone();
                                new_p.children.insert(
                                    0,
                                    ParagraphChild::Run(Box::new(
                                        Run::new().add_field_char(FieldCharType::End, false),
                                    )),
                                );
                                b = b.add_child(&new_p)?
                            } else {
                                b = b.add_child(&p)?;
                            }
                        }
                        TocContent::Table(t) => {
                            // insert empty line for table
                            // because it causes docx error if table is added after TOC
                            if i == 0 {
                                b = b.add_child(
                                    &Paragraph::new().add_run(Run::new().add_text("")),
                                )?;
                            }
                            b = b.add_child(&t)?;
                        }
                    }
                }
            }

            if !self.without_sdt {
                b = b.close()?.close()?;
            }

            b.into_inner()
        } else {
            let items: Vec<TableOfContentsItem> = self
                .items
                .iter()
                .map(|item| {
                    let mut item = item.clone();
                    item.instr = self.instr.clone();
                    item.dirty = self.dirty;
                    if item.page_ref.is_none() {
                        item.page_ref = self.page_ref_placeholder.clone();
                    }
                    item
                })
                .collect();

            let mut b = XMLBuilder::from(stream);

            if !self.without_sdt {
                b = b
                    .open_structured_tag()?
                    .add_child(&p)?
                    .open_structured_tag_content()?;
            }

            for c in self.before_contents.iter() {
                match c {
                    TocContent::Paragraph(p) => {
                        b = b.add_child(&p)?;
                    }
                    TocContent::Table(t) => {
                        b = b.add_child(&t)?;
                    }
                }
            }

            b = b.add_child(&items)?;

            for (i, c) in self.after_contents.iter().enumerate() {
                match c {
                    TocContent::Paragraph(p) => {
                        b = b.add_child(&p)?;
                    }
                    TocContent::Table(t) => {
                        // insert empty line for table
                        // because it causes docx error if table is added after TOC
                        if i == 0 {
                            b = b.add_child(&Paragraph::new().add_run(Run::new().add_text("")))?;
                        }
                        b = b.add_child(&t)?;
                    }
                }
            }

            if !self.without_sdt {
                b = b.close()?.close()?;
            }
            b.into_inner()
        }
    }
}

