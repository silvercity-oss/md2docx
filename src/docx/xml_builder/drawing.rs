use super::XMLBuilder;
use super::XmlEvent;

use std::io::Write;

impl<W: Write> XMLBuilder<W> {
    open!(open_wp_inline, "wp:inline", "distT", "distB", "distL", "distR");

    open!(
        open_wp_anchor,
        "wp:anchor",
        "distT",
        "distB",
        "distL",
        "distR",
        "simplePos",
        "allowOverlap",
        "behindDoc",
        "locked",
        "layoutInCell",
        "relativeHeight"
    );

    open!(open_a_graphic, "a:graphic", "xmlns:a");
    open!(open_a_graphic_data, "a:graphicData", "uri");
    closed!(wp_extent, "wp:extent", "cx", "cy");
    closed!(wp_effect_extent, "wp:effectExtent", "b", "l", "r", "t");
    closed!(wp_doc_pr, "wp:docPr", "id", "name");
    /// `wp:docPr` with the accessibility description and title. Values are XML-escaped here.
    pub(crate) fn wp_doc_pr_described(
        self,
        id: usize,
        name: &str,
        descr: &str,
        title: &str,
    ) -> crate::docx::xml::writer::Result<Self> {
        let name = crate::docx::escape::escape(name);
        let mut e = XmlEvent::start_element("wp:docPr")
            .attr_display("id", id)
            .attr("name", &name);
        let descr = crate::docx::escape::escape(descr);
        let title = crate::docx::escape::escape(title);
        if !descr.is_empty() {
            e = e.attr("descr", &descr);
        }
        if !title.is_empty() {
            e = e.attr("title", &title);
        }
        self.write(e)?.close()
    }

    /// `pic:cNvPr` carrying the same description.
    pub(crate) fn pic_c_nv_pr_described(self, name: &str, descr: &str) -> crate::docx::xml::writer::Result<Self> {
        let name = crate::docx::escape::escape(name);
        let mut e = XmlEvent::start_element("pic:cNvPr").attr("id", "0").attr("name", &name);
        let descr = crate::docx::escape::escape(descr);
        if !descr.is_empty() {
            e = e.attr("descr", &descr);
        }
        self.write(e)?.close()
    }

    open!(open_wp_c_nv_graphic_frame_pr, "wp:cNvGraphicFramePr");
    closed!(
        a_graphic_frame_locks,
        "a:graphicFrameLocks",
        "xmlns:a",
        "noChangeAspect"
    );

    closed!(simple_pos, "wp:simplePos", "x", "y");
    open!(open_position_h, "wp:positionH", "relativeFrom");
    open!(open_position_v, "wp:positionV", "relativeFrom");
    closed_with_child!(pos_offset, "wp:posOffset");
    closed_with_child!(align, "wp:align");
    closed!(wrap_none, "wp:wrapNone");
    closed!(wrap_square, "wp:wrapSquare", "wrapText");
}
