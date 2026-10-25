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
