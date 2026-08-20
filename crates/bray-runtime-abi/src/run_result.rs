use std::mem::{align_of, size_of};

/// Target-native layout used to move one terminal outcome into a Bray `RunResult<T>`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeRunResultLayout {
    size: usize,
    alignment: usize,
    tag_size: usize,
    completed_tag: u64,
    completed_offset: usize,
    completed_size: usize,
    completed_alignment: usize,
    panicked_tag: u64,
    panicked_offset: usize,
    cancelled_tag: u64,
}

impl NativeRunResultLayout {
    /// Creates one closed run-result layout from compiler-selected union metadata.
    #[expect(
        clippy::too_many_arguments,
        reason = "the native transfer contract carries every independently validated layout field"
    )]
    pub const fn new(
        size: usize,
        alignment: usize,
        tag_size: usize,
        completed_tag: u64,
        completed_offset: usize,
        completed_size: usize,
        completed_alignment: usize,
        panicked_tag: u64,
        panicked_offset: usize,
        cancelled_tag: u64,
    ) -> Self {
        Self {
            size,
            alignment,
            tag_size,
            completed_tag,
            completed_offset,
            completed_size,
            completed_alignment,
            panicked_tag,
            panicked_offset,
            cancelled_tag,
        }
    }

    /// Returns the total represented byte size.
    pub const fn size(self) -> usize {
        self.size
    }

    /// Returns the required represented alignment.
    pub const fn alignment(self) -> usize {
        self.alignment
    }

    /// Returns the represented tag byte width.
    pub const fn tag_size(self) -> usize {
        self.tag_size
    }

    /// Returns the completed variant tag.
    pub const fn completed_tag(self) -> u64 {
        self.completed_tag
    }

    /// Returns the completed payload byte offset.
    pub const fn completed_offset(self) -> usize {
        self.completed_offset
    }

    /// Returns the completed payload byte size.
    pub const fn completed_size(self) -> usize {
        self.completed_size
    }

    /// Returns the required alignment of the completed payload.
    pub const fn completed_alignment(self) -> usize {
        self.completed_alignment
    }

    /// Returns the panicked variant tag.
    pub const fn panicked_tag(self) -> u64 {
        self.panicked_tag
    }

    /// Returns the panic-report payload byte offset.
    pub const fn panicked_offset(self) -> usize {
        self.panicked_offset
    }

    /// Returns the cancelled variant tag.
    pub const fn cancelled_tag(self) -> u64 {
        self.cancelled_tag
    }

    /// Returns whether every offset, size, and scalar field is internally consistent.
    pub const fn is_valid(self) -> bool {
        self.alignment.is_power_of_two()
            && self.alignment >= align_of::<usize>()
            && matches!(self.tag_size, 1 | 2 | 4 | 8)
            && tags_fit(
                self.tag_size,
                [self.completed_tag, self.panicked_tag, self.cancelled_tag],
            )
            && tags_are_distinct([self.completed_tag, self.panicked_tag, self.cancelled_tag])
            && self.tag_size <= self.size
            && self.completed_offset >= self.tag_size
            && self.completed_size <= self.size.saturating_sub(self.completed_offset)
            && self.completed_alignment.is_power_of_two()
            && self.completed_alignment <= self.alignment
            && self
                .completed_offset
                .is_multiple_of(self.completed_alignment)
            && self.panicked_offset >= self.tag_size
            && self.panicked_offset.is_multiple_of(align_of::<usize>())
            && size_of::<usize>() <= self.size.saturating_sub(self.panicked_offset)
    }
}

const fn tags_fit(size: usize, tags: [u64; 3]) -> bool {
    let maximum = match size {
        1 => u8::MAX as u64,
        2 => u16::MAX as u64,
        4 => u32::MAX as u64,
        8 => u64::MAX,
        _ => return false,
    };

    tags[0] <= maximum && tags[1] <= maximum && tags[2] <= maximum
}

const fn tags_are_distinct(tags: [u64; 3]) -> bool {
    tags[0] != tags[1] && tags[0] != tags[2] && tags[1] != tags[2]
}

#[cfg(test)]
mod tests {
    use super::NativeRunResultLayout;

    #[test]
    fn run_result_layout_has_the_native_abi_layout() {
        assert_abi_layout!(NativeRunResultLayout, size: 80, align: 8, fields: {
            size: 0,
            alignment: 8,
            tag_size: 16,
            completed_tag: 24,
            completed_offset: 32,
            completed_size: 40,
            completed_alignment: 48,
            panicked_tag: 56,
            panicked_offset: 64,
            cancelled_tag: 72,
        });
    }

    #[test]
    fn run_result_layout_requires_distinct_representable_tags_and_bounded_payloads() {
        assert!(layout(1, 2, 3, 8, 8).is_valid());
        assert!(!layout(1, 1, 3, 8, 8).is_valid());
        assert!(!layout(1, 2, 256, 8, 8).is_valid());
        assert!(!layout(1, 2, 3, 0, 8).is_valid());
        assert!(!layout(1, 2, 3, 8, 16).is_valid());

        let misaligned = NativeRunResultLayout::new(24, 8, 1, 1, 4, 8, 8, 2, 16, 3);

        assert!(!misaligned.is_valid());
    }

    const fn layout(
        completed_tag: u64,
        panicked_tag: u64,
        cancelled_tag: u64,
        completed_offset: usize,
        panicked_offset: usize,
    ) -> NativeRunResultLayout {
        NativeRunResultLayout::new(
            16,
            8,
            1,
            completed_tag,
            completed_offset,
            8,
            8,
            panicked_tag,
            panicked_offset,
            cancelled_tag,
        )
    }
}
