use std::mem::{align_of, size_of};

#[derive(Clone, Copy)]
pub(super) struct MemoryRegion {
    start: usize,
    end: usize,
}

impl MemoryRegion {
    pub(super) fn read<T>(pointer: *const T, count: usize) -> Option<Self> {
        Self::new(
            pointer.cast(),
            count.checked_mul(size_of::<T>())?,
            align_of::<T>(),
        )
    }

    pub(super) fn write<T>(pointer: *mut T) -> Option<Self> {
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

    pub(super) const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

pub(super) fn disjoint(regions: &[MemoryRegion]) -> bool {
    regions.iter().enumerate().all(|(index, region)| {
        regions
            .iter()
            .skip(index + 1)
            .all(|other| !region.overlaps(*other))
    })
}

pub(super) fn mutually_disjoint(left: &[MemoryRegion], right: &[MemoryRegion]) -> bool {
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
