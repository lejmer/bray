/// The work selected when an inactive frame adapter enters a run.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeFrameEntry {
    /// Transfer ownership and execute the callable's ordinary body.
    Body = 0,
    /// Transfer ownership and resolve captured values without executing the body.
    CaptureCleanup = 1,
    /// Borrow the original context while captured values become quiescent.
    CaptureQuiescence = 2,
    /// Consume quiescent captured values through synchronous abandonment destruction.
    CaptureDestruction = 3,
}

impl NativeFrameEntry {
    /// Returns the private execution ABI discriminator.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a supported entry discriminator.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Body),
            1 => Some(Self::CaptureCleanup),
            2 => Some(Self::CaptureQuiescence),
            3 => Some(Self::CaptureDestruction),
            _ => None,
        }
    }

    /// Returns the stable inspection name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::CaptureCleanup => "capture_cleanup",
            Self::CaptureQuiescence => "capture_quiescence",
            Self::CaptureDestruction => "capture_destruction",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeFrameEntry;

    #[test]
    fn entry_discriminators_are_closed_and_have_distinct_names() {
        let expected = [
            "body",
            "capture_cleanup",
            "capture_quiescence",
            "capture_destruction",
        ];

        for code in 0..=u8::MAX {
            match NativeFrameEntry::from_code(code) {
                Some(entry) => {
                    assert_eq!(entry.code(), code);
                    assert_eq!(entry.as_str(), expected[usize::from(code)]);
                }
                None => assert!(usize::from(code) >= expected.len()),
            }
        }
    }
}
