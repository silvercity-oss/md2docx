use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub enum InstrText {
    TOC(InstrToC),
    TC(InstrTC),
    PAGE(InstrPAGE),
    NUMPAGES(InstrNUMPAGES),
    PAGEREF(InstrPAGEREF),
    HYPERLINK(InstrHyperlink),
    Unsupported(String),
}

impl BuildXML for Box<InstrText> {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_instr_text()?
            .apply(|b| match self.as_ref() {
                InstrText::TOC(toc) => b.add_child(toc),
                InstrText::TC(tc) => b.add_child(tc),
                InstrText::PAGEREF(page_ref) => b.add_child(page_ref),
                InstrText::PAGE(page) => b.add_child(page),
                InstrText::NUMPAGES(page) => b.add_child(page),
                InstrText::HYPERLINK(_link) => todo!(),
                InstrText::Unsupported(s) => b.plain_text(s),
            })?
            .close()?
            .into_inner()
    }
}
