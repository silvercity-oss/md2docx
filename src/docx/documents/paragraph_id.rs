use super::ids;

/// The next `w14:paraId`, from the conversion's own counter (see [`ids`]). Word requires a value
/// below 0x80000000.
pub fn generate_para_id() -> String {
    format!("{:08x}", ids::para() & 0x7fff_ffff)
}
