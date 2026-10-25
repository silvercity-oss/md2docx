pub fn generate_header_id() -> usize {
    super::ids::header()
}

pub fn create_header_rid(id: usize) -> String {
    format!("rIdHeader{id}")
}
