use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub id: String,
    pub children: Vec<ParagraphChild>,
    pub property: ParagraphProperty,
    pub has_numbering: bool,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            id: crate::docx::generate_para_id(),
            children: Vec::new(),
            property: ParagraphProperty::new(),
            has_numbering: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParagraphChild {
    Run(Box<Run>),
    Insert(Insert),
    Delete(Delete),
    MoveFrom(MoveFrom),
    MoveTo(MoveTo),
    BookmarkStart(BookmarkStart),
    Hyperlink(Hyperlink),
    BookmarkEnd(BookmarkEnd),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
    StructuredDataTag(Box<StructuredDataTag>),
    PageNum(Box<PageNum>),
    NumPages(Box<NumPages>),
}

impl BuildXML for ParagraphChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            ParagraphChild::Run(v) => v.build_to(stream),
            ParagraphChild::Insert(v) => v.build_to(stream),
            ParagraphChild::Delete(v) => v.build_to(stream),
            ParagraphChild::MoveFrom(v) => v.build_to(stream),
            ParagraphChild::MoveTo(v) => v.build_to(stream),
            ParagraphChild::Hyperlink(v) => v.build_to(stream),
            ParagraphChild::BookmarkStart(v) => v.build_to(stream),
            ParagraphChild::BookmarkEnd(v) => v.build_to(stream),
            ParagraphChild::CommentStart(v) => v.build_to(stream),
            ParagraphChild::CommentEnd(v) => v.build_to(stream),
            ParagraphChild::StructuredDataTag(v) => v.build_to(stream),
            ParagraphChild::PageNum(v) => v.build_to(stream),
            ParagraphChild::NumPages(v) => v.build_to(stream),
        }
    }
}

impl Paragraph {
    pub fn new() -> Paragraph {
        Default::default()
    }

    pub fn add_run(mut self, run: Run) -> Paragraph {
        self.children.push(ParagraphChild::Run(Box::new(run)));
        self
    }

    pub(crate) fn unshift_run(mut self, run: Run) -> Paragraph {
        self.children.insert(0, ParagraphChild::Run(Box::new(run)));
        self
    }

    /// Wraps the existing paragraph contents in matching bookmark markers.
    ///
    /// Mutating in place lets automatic TOC expansion bookmark headings
    /// without moving each paragraph out of the document tree.
    pub(crate) fn wrap_by_bookmark(&mut self, id: usize, name: impl Into<String>) {
        self.children
            .insert(0, ParagraphChild::BookmarkStart(BookmarkStart::new(id, name)));
        self.children.push(ParagraphChild::BookmarkEnd(BookmarkEnd::new(id)));
    }

    pub fn add_hyperlink(mut self, link: Hyperlink) -> Self {
        self.children.push(ParagraphChild::Hyperlink(link));
        self
    }

    pub fn add_delete(mut self, delete: Delete) -> Paragraph {
        self.children.push(ParagraphChild::Delete(delete));
        self
    }

    pub fn add_bookmark_start(mut self, id: usize, name: impl Into<String>) -> Paragraph {
        self.children
            .push(ParagraphChild::BookmarkStart(BookmarkStart::new(id, name)));
        self
    }

    pub fn add_bookmark_end(mut self, id: usize) -> Paragraph {
        self.children.push(ParagraphChild::BookmarkEnd(BookmarkEnd::new(id)));
        self
    }

    pub fn align(mut self, alignment_type: AlignmentType) -> Paragraph {
        self.property = self.property.align(alignment_type);
        self
    }

    pub fn style(mut self, style_id: &str) -> Paragraph {
        self.property = self.property.style(style_id);
        self
    }

    pub fn add_tab(mut self, t: Tab) -> Self {
        self.property = self.property.add_tab(t);
        self
    }

    pub fn indent(
        mut self,
        left: Option<i32>,
        special_indent: Option<SpecialIndentType>,
        end: Option<i32>,
        start_chars: Option<i32>,
    ) -> Paragraph {
        self.property = self.property.indent(left, special_indent, end, start_chars);
        self
    }

    pub fn numbering(mut self, id: NumberingId, level: IndentLevel) -> Self {
        self.property = self.property.numbering(id, level);
        self.has_numbering = true;
        self
    }

    pub fn run_property(mut self, p: RunProperty) -> Self {
        self.property.run_property = p;
        self
    }

    pub fn line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.property = self.property.line_spacing(spacing);
        self
    }

    // internal
    pub(crate) fn paragraph_property(mut self, p: ParagraphProperty) -> Self {
        self.property = p;
        self
    }

    pub fn raw_text(&self) -> String {
        let mut s = "".to_string();
        // For now support only run, ins, and moveTo.
        // MoveFrom is skipped — it contains ghost text from the original location.
        for c in self.children.iter() {
            match c {
                ParagraphChild::Insert(i) => {
                    for c in i.children.iter() {
                        if let InsertChild::Run(r) = c {
                            for c in r.children.iter() {
                                match c {
                                    RunChild::Text(t) => {
                                        s.push_str(&t.text);
                                    }
                                    RunChild::Tab(_t) => {
                                        s.push('\t');
                                    }
                                    RunChild::PTab(_t) => {
                                        s.push('\t');
                                    }
                                    RunChild::Break(_b) => {
                                        s.push('\n');
                                    }
                                    RunChild::CarriageReturn(_cr) => {
                                        s.push('\n');
                                    }
                                    RunChild::NoBreakHyphen(_h) => {
                                        s.push('\u{2011}');
                                    }
                                    RunChild::SoftHyphen(_h) => {
                                        s.push('\u{00AD}');
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                ParagraphChild::Run(run) => {
                    for c in run.children.iter() {
                        match c {
                            RunChild::Text(t) => {
                                s.push_str(&t.text);
                            }
                            RunChild::Tab(_t) => {
                                s.push('\t');
                            }
                            RunChild::PTab(_t) => {
                                s.push('\t');
                            }
                            RunChild::Break(_b) => {
                                s.push('\n');
                            }
                            RunChild::CarriageReturn(_cr) => {
                                s.push('\n');
                            }
                            RunChild::NoBreakHyphen(_h) => {
                                s.push('\u{2011}');
                            }
                            RunChild::SoftHyphen(_h) => {
                                s.push('\u{00AD}');
                            }
                            _ => {}
                        }
                    }
                }
                ParagraphChild::MoveTo(mt) => {
                    for c in mt.children.iter() {
                        if let MoveToChild::Run(r) = c {
                            for c in r.children.iter() {
                                if let RunChild::Text(t) = c {
                                    s.push_str(&t.text);
                                }
                            }
                        }
                    }
                }
                ParagraphChild::MoveFrom(_) => {
                    // Skip — ghost text from original location
                }
                _ => {}
            }
        }
        s
    }

    pub fn add_page_num(mut self, p: PageNum) -> Self {
        self.children.push(ParagraphChild::PageNum(Box::new(p)));
        self
    }

    // frameProperty
}

impl BuildXML for Paragraph {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_paragraph(&self.id)?
            .add_child(&self.property)?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}
