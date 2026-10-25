use super::XMLBuilder;
use super::XmlEvent;
use std::io::Write;

impl<W: Write> XMLBuilder<W> {
    // Build RelationShips element
    // i.e. <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
    open!(open_relationships, "Relationships", "xmlns");

    // Build Relationship
    closed!(relationship, "Relationship", "Id", "Type", "Target");
    closed!(
        relationship_with_mode,
        "Relationship",
        "Id",
        "Type",
        "Target",
        "TargetMode"
    );
}
