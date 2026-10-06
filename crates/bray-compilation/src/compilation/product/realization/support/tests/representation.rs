use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{CodegenFieldLayout, CodegenTypeKind, TargetAddressSpaceKind};
use bray_compiler_known::RepresentationRole;
use bray_symbols::testing::intern_type;
use bray_symbols::{BorrowKind, NamedTypeSymbolId, SymbolOrigin, TraitApplicationData, TypeData};
use bray_target::TargetValueLayout;

use super::targets::{baseline_codegen_target, codegen_target};
use super::type_fixtures::{realized_types, source_union_type};

use crate::CancellationToken;
use crate::compilation::product::realization::support::pointer_layout;
use crate::compilation::substitution::{empty_substitution, named_type};
use crate::test_support::compilation;

#[test]
fn nullable_and_union_codegen_use_checked_payload_layouts() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@layout(stable, tag = u8)\n",
        "union Choice\n",
        "{\n",
        "    @tag(3)\n",
        "    Value(value: i32);\n",
        "    @tag(7)\n",
        "    Empty;\n",
        "}\n",
        "func main() {}\n",
    ));

    let target = codegen_target(&compilation);
    let union = source_union_type(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let nullable = values
        .intern_type(TypeData::Nullable(union))
        .expect("nullable union type must intern");

    let mut mappings = BTreeMap::new();
    let mut pending = BTreeSet::new();

    compilation
        .codegen_type(
            nullable,
            &target,
            &CancellationToken::new(),
            &mut mappings,
            &mut pending,
        )
        .expect("nullable union must realize");

    let nullable = mappings
        .get(&nullable)
        .expect("nullable mapping must be present");

    assert!(matches!(
        nullable.kind(),
        CodegenTypeKind::Aggregate(fields) if fields.len() == 2
    ));

    let union = mappings.get(&union).expect("union mapping must be present");

    assert!(matches!(
        union.kind(),
        CodegenTypeKind::Union { variants, .. }
            if variants
                .iter()
                .map(|variant| variant.tag().and_then(|tag| tag.to_u64()))
                .collect::<Vec<_>>()
                == [Some(3), Some(7)]
    ));
}

#[test]
fn compiler_known_pointers_retain_their_semantic_pointee_and_address_space() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let element = compilation
        .compiler_known_type(RepresentationRole::ScalarU8)
        .expect("byte representation must resolve");

    let pointer = |role| {
        compilation
            .available_compiler_known_symbols()
            .unary_representation_type(values, role, element)
            .unwrap_or_else(|error| panic!("{role:?} pointer type must intern: {error:?}"))
            .unwrap_or_else(|| panic!("{role:?} pointer type must resolve"))
    };

    let raw = pointer(RepresentationRole::RawPointer);
    let device = pointer(RepresentationRole::DevicePointer);
    let mappings = realized_types(&compilation, &target, [raw, device]);

    assert!(matches!(
        mappings[&raw].kind(),
        CodegenTypeKind::Pointer {
            target,
            address_space: TargetAddressSpaceKind::Default,
        } if *target == element
    ));

    assert!(matches!(
        mappings[&device].kind(),
        CodegenTypeKind::Pointer {
            target,
            address_space: TargetAddressSpaceKind::Device,
        } if *target == element
    ));
}

#[test]
fn panic_report_semantic_layout_matches_the_native_report() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let report = compilation
        .codegen_representation_type(RepresentationRole::PanicReport)
        .expect("panic report representation must realize");

    let mappings = realized_types(&compilation, &target, [report]);

    let layout = mappings[&report]
        .layout()
        .expect("panic report must be sized");

    assert_eq!(
        layout.size(),
        std::mem::size_of::<bray_runtime_abi::NativePanicReport>() as u64
    );

    assert_eq!(
        layout.alignment().get(),
        std::mem::align_of::<bray_runtime_abi::NativePanicReport>() as u64
    );
}

#[test]
fn sized_boxes_preserve_the_storage_policy_representation() {
    let compilation = compilation("module app;");
    let target = baseline_codegen_target();
    let values = compilation.semantic_value_store().unwrap();

    let scalar = compilation
        .compiler_known_type(RepresentationRole::ScalarI64)
        .unwrap();

    let policy = intern_type(values, TypeData::tuple([scalar, scalar, scalar]));

    let owned = intern_type(
        values,
        TypeData::OwnedIndirection {
            storage: policy,
            target: scalar,
        },
    );

    let mappings = realized_types(&compilation, &target, [owned]);

    assert_eq!(
        mappings[&owned],
        mappings[&policy].representation_for(owned)
    );

    assert_eq!(
        mappings[&owned].layout().map(TargetValueLayout::size),
        Some(24)
    );
}

#[test]
fn unsized_subjects_receive_layout_only_at_indirection_boundaries() {
    let compilation = compilation("module app;\ntrait Marker {}\n");
    let target = baseline_codegen_target();

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let scalar = compilation
        .compiler_known_type(RepresentationRole::ScalarI32)
        .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

    let slice = intern_type(values, TypeData::Slice(scalar));

    let borrowed_slice = intern_type(
        values,
        TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: slice,
        },
    );

    let owned_slice = intern_type(
        values,
        TypeData::OwnedIndirection {
            storage: scalar,
            target: slice,
        },
    );

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let marker = symbols
        .traits()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("fixture must declare Marker"));

    let substitution = empty_substitution(values, marker.id().into())
        .unwrap_or_else(|error| panic!("trait substitution must be available: {error:?}"));

    let application = values
        .intern_trait_application(TraitApplicationData::new(marker.id(), substitution))
        .unwrap_or_else(|error| panic!("trait application must be valid: {error:?}"));

    let view = intern_type(values, TypeData::TraitView(application));

    let borrowed_view = intern_type(
        values,
        TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: view,
        },
    );

    let owned_view = intern_type(
        values,
        TypeData::OwnedIndirection {
            storage: scalar,
            target: view,
        },
    );

    let mappings = realized_types(
        &compilation,
        &target,
        [borrowed_slice, owned_slice, borrowed_view, owned_view],
    );

    assert!(mappings[&slice].layout().is_none());

    assert!(matches!(
        mappings[&slice].kind(),
        CodegenTypeKind::UnsizedSlice { element } if *element == scalar
    ));

    assert!(mappings[&view].layout().is_none());

    assert!(matches!(
        mappings[&view].kind(),
        CodegenTypeKind::UnsizedTraitView
    ));

    for boundary in [borrowed_slice, owned_slice, borrowed_view, owned_view] {
        let mapping = &mappings[&boundary];

        assert_eq!(
            mapping.layout().map(TargetValueLayout::size),
            Some(pointer_layout(&target).size() * 2)
        );

        assert!(matches!(
            mapping.kind(),
            CodegenTypeKind::Aggregate(fields) if fields.len() == 2
        ));
    }
}

#[test]
fn special_values_use_component_and_metadata_layouts() {
    let compilation = compilation("module app;\n");
    let target = baseline_codegen_target();

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let scalar = compilation
        .compiler_known_type(RepresentationRole::ScalarI32)
        .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

    let real = compilation
        .compiler_known_type(RepresentationRole::ScalarR64)
        .unwrap_or_else(|error| panic!("r64 must be available: {error:?}"));

    let complex = compilation
        .compiler_known_type(RepresentationRole::ScalarC128)
        .unwrap_or_else(|error| panic!("c128 must be available: {error:?}"));

    let nullable = intern_type(values, TypeData::Nullable(scalar));
    let generator = intern_type(values, TypeData::Generator(scalar));

    let mappings = realized_types(&compilation, &target, [complex, nullable, generator]);

    let CodegenTypeKind::Aggregate(complex_fields) = mappings[&complex].kind() else {
        panic!("complex values must map to their two real components");
    };

    assert_eq!(
        complex_fields
            .iter()
            .map(|field| (field.ty(), field.offset_bytes()))
            .collect::<Vec<_>>(),
        [(real, 0), (real, 8)]
    );

    assert_eq!(
        mappings[&complex].layout().map(TargetValueLayout::size),
        Some(16)
    );

    let CodegenTypeKind::Aggregate(nullable_fields) = mappings[&nullable].kind() else {
        panic!("nullable values must map to state and payload");
    };

    assert_eq!(
        nullable_fields
            .iter()
            .map(CodegenFieldLayout::offset_bytes)
            .collect::<Vec<_>>(),
        [0, 4]
    );

    assert_eq!(
        mappings[&nullable].layout().map(TargetValueLayout::size),
        Some(8)
    );

    assert_eq!(
        mappings[&generator].layout().map(TargetValueLayout::size),
        Some(pointer_layout(&target).size() * 3)
    );
}

#[test]
fn union_realization_uses_checked_tags_and_payload_layouts() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    Empty;\n",
        "    Number(value: i64);\n",
        "    Pair(small: i8, large: i64);\n",
        "}\n",
    ));

    let target = baseline_codegen_target();

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

    let union = symbols
        .unions()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        .unwrap_or_else(|| panic!("fixture must declare Choice"));

    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

    let ty = named_type(values, NamedTypeSymbolId::Union(union.id()))
        .unwrap_or_else(|error| panic!("union type must be available: {error:?}"));

    let mappings = realized_types(&compilation, &target, [ty]);
    let mapping = &mappings[&ty];

    let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
        panic!("Choice must map to a tagged union");
    };

    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.tag().and_then(|tag| tag.to_u64()))
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)]
    );

    assert_eq!(
        variants
            .iter()
            .map(|variant| {
                variant
                    .fields()
                    .iter()
                    .map(CodegenFieldLayout::offset_bytes)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        [vec![], vec![8], vec![8, 16]]
    );

    let tag = tag.unwrap_or_else(|| panic!("Choice must include tag storage"));

    assert_eq!(
        mappings[&tag].layout().map(TargetValueLayout::size),
        Some(1)
    );

    assert_eq!(mapping.layout().map(TargetValueLayout::size), Some(24));
}
