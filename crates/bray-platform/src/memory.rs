use std::num::NonZeroUsize;

use memmap2::{Mmap, MmapMut, MmapOptions};

use crate::{PlatformError, PlatformOperation};

/// Intended use of one anonymous native memory mapping.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeMemoryKind {
    /// General runtime-owned data.
    Data,
    /// Native thread stack storage.
    Stack,
}

/// Owned writable anonymous virtual-memory mapping.
#[derive(Debug)]
pub struct NativeMemoryRegion {
    mapping: MmapMut,
    kind: NativeMemoryKind,
}

impl NativeMemoryRegion {
    /// Acquires a zero-initialized anonymous mapping from the host.
    pub fn acquire(
        length: NonZeroUsize,
        kind: NativeMemoryKind,
    ) -> Result<Self, PlatformError> {
        let mut options = MmapOptions::new();

        options.len(length.get());

        if kind == NativeMemoryKind::Stack {
            options.stack();
        }

        let mapping = options.map_anon().map_err(|error| {
            PlatformError::from_io(PlatformOperation::VirtualMemory, &error)
        })?;

        Ok(Self { mapping, kind })
    }

    /// Returns the mapping's intended use.
    pub const fn kind(&self) -> NativeMemoryKind {
        self.kind
    }

    /// Returns the mapped byte length.
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// Returns whether the mapping has no addressable bytes.
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    /// Borrows the mapped bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.mapping
    }

    /// Mutably borrows the mapped bytes.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.mapping
    }

    /// Removes write access and returns an immutable mapping.
    pub fn make_read_only(self) -> Result<NativeReadOnlyMemory, PlatformError> {
        let mapping = self.mapping.make_read_only().map_err(|error| {
            PlatformError::from_io(PlatformOperation::VirtualMemory, &error)
        })?;

        Ok(NativeReadOnlyMemory {
            mapping,
            kind: self.kind,
        })
    }
}

/// Owned immutable anonymous virtual-memory mapping.
#[derive(Debug)]
pub struct NativeReadOnlyMemory {
    mapping: Mmap,
    kind: NativeMemoryKind,
}

impl NativeReadOnlyMemory {
    /// Returns the mapping's intended use.
    pub const fn kind(&self) -> NativeMemoryKind {
        self.kind
    }

    /// Borrows the mapped bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.mapping
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{NativeMemoryKind, NativeMemoryRegion};

    #[test]
    fn anonymous_mappings_are_zeroed_writable_and_freezable() {
        let Some(length) = NonZeroUsize::new(4096) else {
            panic!("test mapping length must be nonzero");
        };

        let mut mapping =
            NativeMemoryRegion::acquire(length, NativeMemoryKind::Data)
                .unwrap_or_else(|error| panic!("mapping must succeed: {error:?}"));

        assert!(mapping.bytes().iter().all(|byte| *byte == 0));

        mapping.bytes_mut()[0] = 42;

        let mapping = mapping
            .make_read_only()
            .unwrap_or_else(|error| panic!("mapping must become read-only: {error:?}"));

        assert_eq!(mapping.bytes()[0], 42);
    }
}
