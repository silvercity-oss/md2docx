pub trait ParagraphPropertyChangeId {
    fn generate(&self) -> String {
        super::ids::history().to_string()
    }
}
