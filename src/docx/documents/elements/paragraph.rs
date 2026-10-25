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

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn children(&self) -> &Vec<ParagraphChild> {
        &self.children
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
        self.children.insert(
            0,
            ParagraphChild::BookmarkStart(BookmarkStart::new(id, name)),
        );
        self.children
            .push(ParagraphChild::BookmarkEnd(BookmarkEnd::new(id)));
    }

    pub fn add_hyperlink(mut self, link: Hyperlink) -> Self {
        self.children.push(ParagraphChild::Hyperlink(link));
        self
    }

    pub fn add_structured_data_tag(mut self, t: StructuredDataTag) -> Self {
        self.children
            .push(ParagraphChild::StructuredDataTag(Box::new(t)));
        self
    }

    pub fn add_insert(mut self, insert: Insert) -> Paragraph {
        self.children.push(ParagraphChild::Insert(insert));
        self
    }

    pub fn add_delete(mut self, delete: Delete) -> Paragraph {
        self.children.push(ParagraphChild::Delete(delete));
        self
    }

    pub fn add_move_from(mut self, move_from: MoveFrom) -> Paragraph {
        self.children.push(ParagraphChild::MoveFrom(move_from));
        self
    }

    pub fn add_move_to(mut self, move_to: MoveTo) -> Paragraph {
        self.children.push(ParagraphChild::MoveTo(move_to));
        self
    }

    pub fn add_bookmark_start(mut self, id: usize, name: impl Into<String>) -> Paragraph {
        self.children
            .push(ParagraphChild::BookmarkStart(BookmarkStart::new(id, name)));
        self
    }

    pub fn add_bookmark_end(mut self, id: usize) -> Paragraph {
        self.children
            .push(ParagraphChild::BookmarkEnd(BookmarkEnd::new(id)));
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> Paragraph {
        self.children.push(ParagraphChild::CommentStart(Box::new(
            CommentRangeStart::new(comment),
        )));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> Paragraph {
        self.children
            .push(ParagraphChild::CommentEnd(CommentRangeEnd::new(id)));
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

    pub fn snap_to_grid(mut self, v: bool) -> Self {
        self.property = self.property.snap_to_grid(v);
        self
    }

    pub fn keep_next(mut self, v: bool) -> Self {
        self.property = self.property.keep_next(v);
        self
    }

    pub fn contextual_spacing(mut self, v: bool) -> Self {
        self.property = self.property.contextual_spacing(v);
        self
    }

    pub fn keep_lines(mut self, v: bool) -> Self {
        self.property = self.property.keep_lines(v);
        self
    }

    pub fn outline_lvl(mut self, v: usize) -> Self {
        self.property = self.property.outline_lvl(v);
        self
    }

    pub fn page_break_before(mut self, v: bool) -> Self {
        self.property = self.property.page_break_before(v);
        self
    }

    pub fn widow_control(mut self, v: bool) -> Self {
        self.property = self.property.widow_control(v);
        self
    }

    pub fn section_property(mut self, s: SectionProperty) -> Self {
        self.property = self.property.section_property(s);
        self
    }

    pub fn add_tab(mut self, t: Tab) -> Self {
        self.property = self.property.add_tab(t);
        self
    }

    pub fn tabs(mut self, tabs: &[Tab]) -> Self {
        for tab in tabs {
            self.property = self.property.add_tab(tab.clone());
        }
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

    pub fn start_chars(mut self, chars: i32) -> Paragraph {
        self.property = self.property.start_chars(chars);
        self
    }

    pub fn end_chars(mut self, chars: i32) -> Paragraph {
        self.property = self.property.end_chars(chars);
        self
    }

    pub fn hanging_chars(mut self, chars: i32) -> Paragraph {
        self.property = self.property.hanging_chars(chars);
        self
    }

    pub fn first_line_chars(mut self, chars: i32) -> Paragraph {
        self.property = self.property.first_line_chars(chars);
        self
    }

    pub fn numbering(mut self, id: NumberingId, level: IndentLevel) -> Self {
        self.property = self.property.numbering(id, level);
        self.has_numbering = true;
        self
    }

    pub fn size(mut self, size: usize) -> Self {
        self.property.run_property = self.property.run_property.size(size);
        self
    }

    pub fn color(mut self, c: impl Into<String>) -> Self {
        self.property.run_property = self.property.run_property.color(c);
        self
    }

    pub fn bold(mut self) -> Self {
        self.property.run_property = self.property.run_property.bold();
        self
    }

    pub fn italic(mut self) -> Self {
        self.property.run_property = self.property.run_property.italic();
        self
    }

    pub fn fonts(mut self, f: RunFonts) -> Self {
        self.property.run_property = self.property.run_property.fonts(f);
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

    pub fn character_spacing(mut self, spacing: i32) -> Self {
        self.property = self.property.character_spacing(spacing);
        self
    }

    pub fn delete(mut self, author: impl Into<String>, date: impl Into<String>) -> Self {
        self.property.run_property.del = Some(Delete::new().author(author).date(date));
        self
    }

    pub fn insert(mut self, author: impl Into<String>, date: impl Into<String>) -> Self {
        self.property.run_property.ins = Some(Insert::new_with_empty().author(author).date(date));
        self
    }

    pub fn paragraph_property_change(mut self, p: ParagraphPropertyChange) -> Self {
        self.property = self.property.paragraph_property_change(p);
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

    pub fn add_num_pages(mut self, p: NumPages) -> Self {
        self.children.push(ParagraphChild::NumPages(Box::new(p)));
        self
    }

    // frameProperty
    pub fn wrap(mut self, wrap: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            wrap: Some(wrap.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn v_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            v_anchor: Some(anchor.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn h_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            h_anchor: Some(anchor.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn h_rule(mut self, r: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            h_rule: Some(r.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn x_align(mut self, align: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            x_align: Some(align.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn y_align(mut self, align: impl Into<String>) -> Self {
        self.property.frame_property = Some(FrameProperty {
            y_align: Some(align.into()),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn h_space(mut self, x: i32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            h_space: Some(x),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn v_space(mut self, x: i32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            v_space: Some(x),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn frame_x(mut self, x: i32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            x: Some(x),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn frame_y(mut self, y: i32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            y: Some(y),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn frame_width(mut self, n: u32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            w: Some(n),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }

    pub fn frame_height(mut self, n: u32) -> Self {
        self.property.frame_property = Some(FrameProperty {
            h: Some(n),
            ..self.property.frame_property.unwrap_or_default()
        });
        self
    }
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

