pub trait TocKeyGenerator {
    fn generate() -> String {
        format!("_Toc{:08}", super::ids::toc())
    }
}

pub struct TocKey {}

impl TocKeyGenerator for TocKey {}
