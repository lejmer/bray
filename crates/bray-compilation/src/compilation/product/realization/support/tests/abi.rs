use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU16, NonZeroU64};

use bray_codegen::{
    CodegenCallableSignature, CodegenFieldLayout, CodegenIndirectParameterKind,
    CodegenParameterMapping, CodegenResultMapping, CodegenTarget, CodegenTypeKind,
    CodegenTypeMapping,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::testing::intern_type;
use bray_symbols::{CallableAbi, TypeData};
use bray_target::{NativeTarget, TargetLayoutContract, TargetValueLayout};

use super::targets::baseline_codegen_target;
use super::type_fixtures::realized_types;

use crate::CancellationToken;
use crate::compilation::CodegenPreparationError;
use crate::compilation::product::realization::support::{
    indirect_abi_value, indirect_parameter_kind,
};
use crate::test_support::compilation;

#[test]
fn bray_abi_passes_large_composites_indirectly_and_rejects_unsized_values() {
    let compilation = compilation("module app;\n");
    let target = baseline_codegen_target();

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let scalar = compilation
        .compiler_known_type(RepresentationRole::ScalarI32)
        .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

    let generator = intern_type(values, TypeData::Generator(scalar));
    let slice = intern_type(values, TypeData::Slice(scalar));
    let mut mappings = realized_types(&compilation, &target, [generator, slice]);
    let mut pending = BTreeSet::new();

    let signature = CodegenCallableSignature::new(
        [CodegenParameterMapping::direct(generator, None, [])],
        CodegenResultMapping::direct(generator, None, []),
        CallableAbi::Bray,
        false,
    );

    let classified = compilation
        .classify_codegen_signature(
            signature,
            &target,
            &CancellationToken::new(),
            &mut mappings,
            &mut pending,
        )
        .unwrap_or_else(|error| panic!("Bray ABI must classify: {error:?}"));

    assert!(matches!(
        classified.parameters(),
        [CodegenParameterMapping::Indirect {
            kind: CodegenIndirectParameterKind::ByValue,
            ..
        }]
    ));

    assert!(matches!(
        classified.result(),
        CodegenResultMapping::Indirect { .. }
    ));

    let unsized_signature = CodegenCallableSignature::new(
        [CodegenParameterMapping::direct(slice, None, [])],
        CodegenResultMapping::Void,
        CallableAbi::Bray,
        false,
    );

    assert_eq!(
        compilation.classify_codegen_signature(
            unsized_signature,
            &target,
            &CancellationToken::new(),
            &mut mappings,
            &mut pending,
        ),
        Err(CodegenPreparationError::UnsizedTypeByValue(slice))
    );
}

#[test]
fn foreign_abi_uses_the_native_aggregate_passing_contract() {
    let aggregate = CodegenTypeKind::aggregate([]);
    let mappings = BTreeMap::new();

    let sixteen_bytes = TargetValueLayout::new(
        16,
        NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
        TargetLayoutContract::Default,
    );

    let windows = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
    let linux = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);
    let aarch64 = CodegenTarget::for_native(NativeTarget::Aarch64LinuxGnu);

    assert!(indirect_abi_value(
        CallableAbi::C,
        &aggregate,
        sixteen_bytes,
        &windows,
        &mappings,
    ));

    assert!(!indirect_abi_value(
        CallableAbi::C,
        &aggregate,
        sixteen_bytes,
        &linux,
        &mappings,
    ));

    assert!(!indirect_abi_value(
        CallableAbi::Bray,
        &aggregate,
        sixteen_bytes,
        &windows,
        &mappings,
    ));

    assert_eq!(
        indirect_parameter_kind(CallableAbi::C, &windows),
        CodegenIndirectParameterKind::Reference
    );

    assert_eq!(
        indirect_parameter_kind(CallableAbi::C, &linux),
        CodegenIndirectParameterKind::ByValue
    );

    assert_eq!(
        indirect_parameter_kind(CallableAbi::C, &aarch64),
        CodegenIndirectParameterKind::Reference
    );

    assert_eq!(
        indirect_parameter_kind(CallableAbi::Bray, &windows),
        CodegenIndirectParameterKind::ByValue
    );
}

#[test]
fn aarch64_foreign_abi_passes_homogeneous_float_aggregates_directly() {
    let compilation = compilation("module app;\n");

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let element = intern_type(values, TypeData::Error);
    let alignment = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
    let element_layout = TargetValueLayout::new(8, alignment, TargetLayoutContract::Default);

    let mappings = BTreeMap::from([(
        element,
        CodegenTypeMapping::new(
            element,
            element_layout,
            CodegenTypeKind::Float(NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)),
        ),
    )]);

    let aggregate = CodegenTypeKind::aggregate([
        CodegenFieldLayout::new(None, element, 0),
        CodegenFieldLayout::new(None, element, 8),
        CodegenFieldLayout::new(None, element, 16),
    ]);

    let aggregate_layout = TargetValueLayout::new(24, alignment, TargetLayoutContract::Default);

    for native in [
        NativeTarget::Aarch64LinuxGnu,
        NativeTarget::Aarch64WindowsMsvc,
        NativeTarget::Aarch64MacOs,
    ] {
        assert!(!indirect_abi_value(
            CallableAbi::C,
            &aggregate,
            aggregate_layout,
            &CodegenTarget::for_native(native),
            &mappings,
        ));
    }

    assert!(indirect_abi_value(
        CallableAbi::C,
        &aggregate,
        aggregate_layout,
        &CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu),
        &mappings,
    ));
}
