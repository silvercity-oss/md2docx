use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

/*
  17.3.2.26 rFonts (Run Fonts)
  This element specifies the fonts which shall be used to display the text contents of this run.
  Within a single run, there can be up to four types of font slot which shall each be allowed to use a unique font:
  - ASCII (i.e., the first 128 Unicode code points)
  - High ANSI
  - Complex Script
*/
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunFonts {
    ascii: Option<String>,
    hi_ansi: Option<String>,
    east_asia: Option<String>,
    cs: Option<String>,
    ascii_theme: Option<String>,
    hi_ansi_theme: Option<String>,
    east_asia_theme: Option<String>,
    cs_theme: Option<String>,
    hint: Option<String>,
}

impl RunFonts {
    pub fn new() -> RunFonts {
        Default::default()
    }

    pub fn ascii(mut self, f: impl Into<String>) -> Self {
        self.ascii = Some(escape_owned(f.into()));
        self
    }

    pub fn hi_ansi(mut self, f: impl Into<String>) -> Self {
        self.hi_ansi = Some(escape_owned(f.into()));
        self
    }

    pub fn east_asia(mut self, f: impl Into<String>) -> Self {
        self.east_asia = Some(escape_owned(f.into()));
        self
    }

    pub fn cs(mut self, f: impl Into<String>) -> Self {
        self.cs = Some(escape_owned(f.into()));
        self
    }
}

impl BuildXML for RunFonts {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .run_fonts(
                self.ascii.as_ref(),
                self.hi_ansi.as_ref(),
                self.cs.as_ref(),
                self.east_asia.as_ref(),
                self.ascii_theme.as_ref(),
                self.hi_ansi_theme.as_ref(),
                self.cs_theme.as_ref(),
                self.east_asia_theme.as_ref(),
                self.hint.as_ref(),
            )?
            .into_inner()
    }
}
