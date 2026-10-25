pub trait HistoryId {
    fn generate(&self) -> String {
        super::ids::history().to_string()
    }
}
