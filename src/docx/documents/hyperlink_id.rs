pub fn generate_hyperlink_id() -> usize {
    super::ids::hyperlink()
}

pub fn create_hyperlink_rid(id: usize) -> String {
    format!("rIdHyperlink{id}")
}
