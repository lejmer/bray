use bray_runtime_abi::{
    NativeCleanupCapacityMetadata, NativeCleanupCapacityMetadataProvider,
    NativeProductHostDescriptor, NativeRuntimeStatus,
};

use crate::native::export::contain_status;

/// Secures a local cleanup bundle in the product's admitted capacity domain.
pub extern "C" fn bray_runtime_cleanup_capacity_admission(
    product: &NativeProductHostDescriptor,
    count: usize,
    metadata: Option<NativeCleanupCapacityMetadataProvider>,
) -> NativeRuntimeStatus {
    contain_status(|| crate::product::cleanup_capacity_admission(product, count, metadata))
}

/// Retires a local cleanup credit after its owner has settled.
pub extern "C" fn bray_runtime_cleanup_capacity_discharge(
    product: &NativeProductHostDescriptor,
    metadata: &NativeCleanupCapacityMetadata,
) -> NativeRuntimeStatus {
    contain_status(|| crate::product::cleanup_capacity_discharge(product, metadata))
}
