mod abi;
mod layout;
mod queries;
mod signatures;
mod symbols;

#[cfg(test)]
mod tests;

pub(super) use abi::{indirect_abi_value, indirect_parameter_kind};
pub(super) use layout::{
    align_to, atomic_representation_for_type, atomic_storage_is_padding_free, atomic_storage_role,
    ensure_target_alignment, packed_alignment, pointer_layout, pointer_mapping, scalar_mapping,
    sized_layout, target_layout_contract,
};
pub(in crate::compilation::product) use queries::closed_array_length;
pub(super) use queries::codegen_checker_error;
pub(super) use signatures::{
    callable_type_signature, is_void_result, operation_result_type, receiver_codegen_type,
    signature_types, synchronous_bray_signature, void_signature,
};
pub(super) use symbols::{
    dependency_symbol, direct_helper_symbol, helper_runtime_symbol, native_boundary_mapping,
    source_backed_symbol_key,
};
