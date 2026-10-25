use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drawing {
    pub data: Option<DrawingData>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawingData {
    Pic(Pic),
    TextBox(TextBox),
}

impl Drawing {
    pub fn new() -> Drawing {
        Default::default()
    }

    pub fn pic(mut self, pic: Pic) -> Drawing {
        self.data = Some(DrawingData::Pic(pic));
        self
    }
}

impl BuildXML for Drawing {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let b = XMLBuilder::from(stream);
        let mut b = b.open_drawing()?;

        match &self.data {
            Some(DrawingData::Pic(p)) => {
                if let DrawingPositionType::Inline = p.position_type {
                    b = b.open_wp_inline(
                        &format!("{}", p.dist_t),
                        &format!("{}", p.dist_b),
                        &format!("{}", p.dist_l),
                        &format!("{}", p.dist_r),
                    )?
                } else {
                    b = b
                        .open_wp_anchor(
                            &format!("{}", p.dist_t),
                            &format!("{}", p.dist_b),
                            &format!("{}", p.dist_l),
                            &format!("{}", p.dist_r),
                            "0",
                            if p.simple_pos { "1" } else { "0" },
                            "0",
                            "0",
                            if p.layout_in_cell { "1" } else { "0" },
                            &format!("{}", p.relative_height),
                        )?
                        .simple_pos(&format!("{}", p.simple_pos_x), &format!("{}", p.simple_pos_y))?
                        .open_position_h(&format!("{}", p.relative_from_h))?;

                    match p.position_h {
                        DrawingPosition::Offset(x) => {
                            let x = format!("{}", x as u32);
                            b = b.pos_offset(&x)?.close()?;
                        }
                        DrawingPosition::Align(x) => {
                            b = b.align(&x.to_string())?.close()?;
                        }
                    }

                    b = b.open_position_v(&format!("{}", p.relative_from_v))?;

                    match p.position_v {
                        DrawingPosition::Offset(y) => {
                            let y = format!("{}", y as u32);
                            b = b.pos_offset(&y)?.close()?;
                        }
                        DrawingPosition::Align(a) => {
                            b = b.align(&a.to_string())?.close()?;
                        }
                    }
                }

                let w = format!("{}", p.size.0);
                let h = format!("{}", p.size.1);
                b = b
                    // Please see 20.4.2.7 extent (Drawing Object Size)
                    // One inch equates to 914400 EMUs and a centimeter is 360000
                    .wp_extent(&w, &h)?
                    .wp_effect_extent("0", "0", "0", "0")?;
                if p.allow_overlap {
                    b = b.wrap_none()?;
                } else if p.position_type == DrawingPositionType::Anchor {
                    b = b.wrap_square("bothSides")?;
                }
                b = b
                    .wp_doc_pr_described(p.doc_pr_id, &format!("Picture {}", p.doc_pr_id), &p.alt, &p.title)?
                    .open_wp_c_nv_graphic_frame_pr()?
                    .a_graphic_frame_locks("http://schemas.openxmlformats.org/drawingml/2006/main", "1")?
                    .close()?
                    .open_a_graphic("http://schemas.openxmlformats.org/drawingml/2006/main")?
                    .open_a_graphic_data("http://schemas.openxmlformats.org/drawingml/2006/picture")?
                    .add_child(&p.clone())?
                    .close()?
                    .close()?;
            }
            Some(DrawingData::TextBox(_t)) => unimplemented!("TODO: Support textBox writer"),
            None => {
                unimplemented!()
            }
        }
        b.close()?.close()?.into_inner()
    }
}
