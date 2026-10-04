//! AC vs battery, for auto-switching profiles before launch.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

pub fn power_source() -> PowerSource {
    todo!()
}
