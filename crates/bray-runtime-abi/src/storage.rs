/// Concrete layout and selected semantic identity of one cleanup storage region.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCleanupStorageShape {
    identity: [u8; 32],
    bytes: usize,
    alignment: usize,
}

impl NativeCleanupStorageShape {
    /// Creates a boundary descriptor. Admission checks its layout before reserving bytes.
    pub const fn new(identity: [u8; 32], bytes: usize, alignment: usize) -> Self {
        Self {
            identity,
            bytes,
            alignment,
        }
    }
}

/// Opaque native lease for one allocation lifetime in the resident cleanup service.
/// Copying this ABI value does not duplicate its ownership or permit repeated activation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCleanupStorageLease {
    domain: usize,
    shape: usize,
    slot: usize,
    epoch: u64,
    address: usize,
}

impl NativeCleanupStorageLease {
    /// Creates caller-owned output storage with no live reservation.
    pub const fn empty() -> Self {
        Self {
            domain: 0,
            shape: 0,
            slot: 0,
            epoch: 0,
            address: 0,
        }
    }

    /// Returns the address carried by this lease, without granting access authority.
    pub const fn address(self) -> usize {
        self.address
    }
}

/// Immutable concrete cleanup requirements borrowed for one admission or discharge call.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeCleanupStorageBundle<'descriptors> {
    records: usize,
    descriptors: *const NativeCleanupStorageShape,
    count: usize,
    lifetime: core::marker::PhantomData<&'descriptors [NativeCleanupStorageShape]>,
}

impl<'descriptors> NativeCleanupStorageBundle<'descriptors> {
    /// Borrows concrete payload requirements and an independent report-record allowance.
    pub fn new(records: usize, descriptors: &'descriptors [NativeCleanupStorageShape]) -> Self {
        Self {
            records,
            descriptors: descriptors.as_ptr(),
            count: descriptors.len(),
            lifetime: core::marker::PhantomData,
        }
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::{NativeCleanupStorageBundle, NativeCleanupStorageLease, NativeCleanupStorageShape};

    #[test]
    fn storage_transport_matches_native_descriptor_and_lease_layouts() {
        assert_abi_layout!(NativeCleanupStorageShape, size: 48, align: 8, fields: {
            identity: 0, bytes: 32, alignment: 40,
        });

        assert_abi_layout!(NativeCleanupStorageBundle<'_>, size: 24, align: 8, fields: {
            records: 0, descriptors: 8, count: 16,
        });

        assert_abi_layout!(NativeCleanupStorageLease, size: 40, align: 8, fields: {
            domain: 0, shape: 8, slot: 16, epoch: 24, address: 32,
        });
    }
}
