use std::mem::{align_of, size_of};

/// Validated half-open native memory range.
#[derive(Clone, Copy)]
pub struct MemoryRegion {
    start: usize,
    end: usize,
}

impl MemoryRegion {
    /// Validates a readable region for a typed element count.
    pub fn read<T>(pointer: *const T, count: usize) -> Option<Self> {
        Self::new(
            pointer.cast(),
            count.checked_mul(size_of::<T>())?,
            align_of::<T>(),
        )
    }

    /// Validates writable storage for one typed value.
    pub fn write<T>(pointer: *mut T) -> Option<Self> {
        Self::new(pointer.cast(), size_of::<T>(), align_of::<T>())
    }

    fn new(pointer: *const u8, bytes: usize, alignment: usize) -> Option<Self> {
        let start = pointer.addr();

        if start % alignment != 0 || bytes != 0 && pointer.is_null() || bytes > isize::MAX as usize
        {
            return None;
        }

        let end = start.checked_add(bytes)?;

        Some(Self { start, end })
    }

    /// Returns whether this region intersects another validated region.
    pub const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Returns whether every region in one collection is pairwise disjoint.
pub fn disjoint(regions: &[MemoryRegion]) -> bool {
    regions.iter().enumerate().all(|(index, region)| {
        regions
            .iter()
            .skip(index + 1)
            .all(|other| !region.overlaps(*other))
    })
}

/// Returns whether no region in the left collection intersects the right collection.
pub fn mutually_disjoint(left: &[MemoryRegion], right: &[MemoryRegion]) -> bool {
    left.iter()
        .all(|left| right.iter().all(|right| !left.overlaps(*right)))
}

#[cfg(test)]
mod tests {
    use super::{MemoryRegion, disjoint, mutually_disjoint};

    #[test]
    fn regions_validate_alignment_bounds_and_overlap() {
        let mut bytes = [0_u64; 2];
        let first = MemoryRegion::write(&mut bytes[0]);
        let second = MemoryRegion::write(&mut bytes[1]);

        let (Some(first), Some(second)) = (first, second) else {
            panic!("aligned test storage must form regions");
        };

        assert!(disjoint(&[first, second]));
        assert!(!disjoint(&[first, first]));
        assert!(mutually_disjoint(&[first], &[second]));
        assert!(!mutually_disjoint(&[first], &[first]));

        assert!(
            MemoryRegion::write(
                (bytes.as_mut_ptr().cast::<u8>())
                    .wrapping_add(1)
                    .cast::<u64>()
            )
            .is_none()
        );

        assert!(MemoryRegion::read(std::ptr::null::<u8>(), 0).is_some());
        assert!(MemoryRegion::read(std::ptr::null::<u8>(), 1).is_none());
    }
}
