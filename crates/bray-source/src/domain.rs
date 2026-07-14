const GENERATED_SOURCE_BIT: u32 = 1 << 31;

pub(crate) const fn stored(raw: u32) -> Option<u32> {
    if is_stored(raw) { Some(raw) } else { None }
}

pub(crate) const fn generated(ordinal: u32) -> Option<u32> {
    if ordinal < GENERATED_SOURCE_BIT {
        Some(GENERATED_SOURCE_BIT | ordinal)
    } else {
        None
    }
}

pub(crate) const fn is_generated(raw: u32) -> bool {
    raw & GENERATED_SOURCE_BIT != 0
}

pub(crate) const fn is_stored(raw: u32) -> bool {
    !is_generated(raw)
}
