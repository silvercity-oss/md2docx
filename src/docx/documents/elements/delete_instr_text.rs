use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub enum DeleteInstrText {
    TOC(InstrToC),
    TC(InstrTC),
    PAGEREF(InstrPAGEREF),
    HYPERLINK(InstrHyperlink),
    Unsupported(String),
}

impl BuildXML for DeleteInstrText {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_delete_instr_text()?
            .apply(|b| match self {
                DeleteInstrText::TOC(toc) => b.add_child(toc),
                DeleteInstrText::TC(tc) => b.add_child(tc),
                DeleteInstrText::PAGEREF(page_ref) => b.add_child(page_ref),
                DeleteInstrText::HYPERLINK(_link) => todo!(),
                DeleteInstrText::Unsupported(s) => b.plain_text(s),
            })?
            .close()?
            .into_inner()
    }
}

