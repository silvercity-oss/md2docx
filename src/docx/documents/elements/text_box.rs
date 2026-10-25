use crate::docx::documents::*;
use crate::docx::types::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    // For writer only
    pub children: Vec<TextBoxContentChild>,
    // unit is emu
    pub size: (u32, u32),
    pub position_type: DrawingPositionType,
    /// Specifies that this object shall be positioned using the positioning information in the
    /// simplePos child element (§20.4.2.13). This positioning, when specified, positions the
    /// object on the page by placing its top left point at the x-y coordinates specified by that
    /// element.
    pub simple_pos: bool,
    // unit is emu
    pub simple_pos_x: i32,
    pub simple_pos_y: i32,
    /// Specifies how this DrawingML object behaves when its anchor is located in a table cell;
    /// and its specified position would cause it to intersect with a table cell displayed in the
    /// document. That behavior shall be as follows:
    pub layout_in_cell: bool,
    /// Specifies the relative Z-ordering of all DrawingML objects in this document. Each floating
    /// DrawingML object shall have a Z-ordering value, which determines which object is
    /// displayed when any two objects intersect. Higher values shall indicate higher Z-order;
    /// lower values shall indicate lower Z-order.
    pub relative_height: u32,
    pub allow_overlap: bool,
    pub position_h: DrawingPosition,
    pub position_v: DrawingPosition,
    pub relative_from_h: RelativeFromHType,
    pub relative_from_v: RelativeFromVType,
    /// Specifies the minimum distance which shall be maintained between the top edge of this drawing object and any subsequent text within the document when this graphical object is displayed within the document's contents.,
    /// The distance shall be measured in EMUs (English Metric Units).,
    pub dist_t: i32,
    pub dist_b: i32,
    pub dist_l: i32,
    pub dist_r: i32,
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBox {
    pub fn new() -> Self {
        Self {
            children: vec![],
            size: (from_px(100), from_px(100)),
            position_type: DrawingPositionType::Inline,
            simple_pos: false,
            simple_pos_x: 0,
            simple_pos_y: 0,
            layout_in_cell: false,
            relative_height: 190500,
            allow_overlap: false,
            position_v: DrawingPosition::Offset(0),
            position_h: DrawingPosition::Offset(0),
            relative_from_h: RelativeFromHType::default(),
            relative_from_v: RelativeFromVType::default(),
            dist_t: 0,
            dist_b: 0,
            dist_l: 0,
            dist_r: 0,
        }
    }

    // unit is emu
}

/*
impl BuildXML for TextBox {
    fn build_to<W: Write>(&self, stream: crate::docx::xml::writer::EventWriter<W>) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let w = format!("{}", self.size.0);
        let h = format!("{}", self.size.1);
        XMLBuilder::from(stream)
            .open_pic("http://schemas.openxmlformats.org/drawingml/2006/picture")?
            .open_pic_nv_pic_pr()?
            .pic_c_nv_pr("0", "")?
            .open_pic_c_nv_pic_pr()?
            .a_pic_locks("1", "1")?
            .close()?
            .close()?
            .open_blip_fill()?
            .a_blip(&self.id)?
            .a_src_rect()?
            .open_a_stretch()?
            .a_fill_rect()?
            .close()?
            .close()?
            .open_pic_sp_pr("auto")?
            .open_a_xfrm()?
            .a_off("0", "0")?
            .a_ext(&w, &h)?
            .close()?
            .open_a_prst_geom("rect")?
            .a_av_lst()?
            .close()?
            .close()?
            .close()?
            .into_inner()
    }
}
*/
