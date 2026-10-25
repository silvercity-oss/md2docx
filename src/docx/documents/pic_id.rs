pub fn generate_pic_id() -> usize {
    super::ids::pic()
}

pub fn create_pic_rid(id: usize) -> String {
    format!("rIdImage{id}")
}
