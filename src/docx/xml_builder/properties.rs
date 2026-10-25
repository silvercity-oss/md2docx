use super::XMLBuilder;
use super::XmlEvent;

use std::io::Write;

impl<W: Write> XMLBuilder<W> {
    // Build Properties element
    // i.e. <Properties xmlns:vt="http://schemas.openxmlformats.org/package/2006/relationships">
    open!(open_properties, "Properties", "xmlns", "xmlns:vt");

    closed_with_child!(template, "Template");
    closed_with_child!(total_time, "TotalTime");
    closed_with_child!(application, "Application");
    closed_with_child!(pages, "Pages");
    closed_with_child!(words, "Words");
    closed_with_child!(characters, "Characters");
    closed_with_child!(characters_with_spaces, "CharactersWithSpaces");
    closed_with_child!(paragraphs, "Paragraphs");
}

