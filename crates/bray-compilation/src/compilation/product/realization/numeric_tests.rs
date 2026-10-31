use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::CallableAbi;

use super::runtime_source::{runtime_source_contract_matches, runtime_source_signature_matches};
use crate::test_support::{compilation, source_function_body_key};
use crate::{CancellationToken, SelectedTarget};

const NUMERIC_ROLES: [RuntimeAbiRole; 4] = [
    RuntimeAbiRole::RealPower,
    RuntimeAbiRole::ComplexMultiply,
    RuntimeAbiRole::ComplexDivide,
    RuntimeAbiRole::ComplexPower,
];

#[test]
fn numeric_runtime_roles_require_one_opaque_entry_authority() {
    for (declarations, requirement, expected) in [
        (
            "trusted predicate ready();",
            "requires(trusted ready())",
            true,
        ),
        ("trusted predicate ready();", "", false),
        (
            "trusted predicate ready(); trusted predicate other();",
            "requires(trusted ready(), trusted other())",
            false,
        ),
        ("predicate ready() = true;", "requires(ready())", false),
        (
            "trusted predicate ready<T>();",
            "requires(trusted ready<u32>())",
            false,
        ),
        (
            "trusted predicate ready(value: u32);",
            "requires(trusted ready(value = 0))",
            false,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            {declarations}
            @abi(c)
            trusted func numeric(
                pos width: u32,
                pos left: DataPointer<u8>,
                pos right: DataPointer<u8>,
                pos output: DataPointer<u8>
            ) {requirement} {{}}
            "#
        ));

        let key = source_function_body_key(&compilation, "numeric");

        for role in NUMERIC_ROLES {
            assert_eq!(
                runtime_source_contract_matches(
                    &compilation,
                    role,
                    &bray_ir::MirUnitKey::Bound(key.clone()),
                    &CancellationToken::new(),
                )
                .expect("numeric entry authority must be checked"),
                expected,
                "{role:?}: {requirement}",
            );
        }
    }
}

#[test]
#[ignore = "tier:exhaustive"]
fn numeric_records_and_native_signatures_agree_for_every_target() {
    let source = include_str!("../../../../../../runtime/bootstrap/src/numeric/records.bray");

    for target in bray_target::NativeTarget::ALL {
        let compilation =
            crate::compilation::trusted_contracts::production_host_compilation_for_target(
                source,
                SelectedTarget::for_native(target),
            );

        let values = compilation.semantic_value_store().expect("numeric values");
        let symbols = compilation.symbol_graph().expect("numeric symbols");

        let records = [("RealBits", 16, vec![0]), ("ComplexBits", 32, vec![0, 16])].map(
            |(name, size, offsets)| {
                let definition = symbols
                    .structures()
                    .iter()
                    .find(|symbol| {
                        symbols
                            .member_name(symbol.id().into())
                            .is_some_and(|actual| actual.as_str() == name)
                    })
                    .expect("production numeric record");

                let ty = crate::compilation::substitution::named_type(
                    values,
                    bray_symbols::NamedTypeSymbolId::Struct(definition.id()),
                )
                .expect("numeric record type");

                (ty, size, offsets)
            },
        );

        let codegen_target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("numeric target supports code generation");

        let mappings = compilation
            .codegen_types(
                records.iter().map(|(ty, _, _)| *ty).collect(),
                None,
                &codegen_target,
                &CancellationToken::new(),
            )
            .expect("numeric record mappings");

        for (ty, size, offsets) in &records {
            let mapping = mappings
                .iter()
                .find(|mapping| mapping.ty() == *ty)
                .expect("numeric record mapping");

            let layout = mapping.layout().expect("numeric record is sized");

            assert_eq!(layout.size(), *size, "{target:?}");
            assert_eq!(layout.alignment().get(), 8, "{target:?}");

            let bray_codegen::CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
                panic!("numeric C record must map to an aggregate");
            };

            assert_eq!(
                fields
                    .iter()
                    .map(|field| field.offset_bytes())
                    .collect::<Vec<_>>(),
                *offsets,
                "{target:?}"
            );
        }

        let width = compilation
            .codegen_representation_type(RepresentationRole::ScalarU32)
            .expect("numeric width representation");

        let wrong_width = compilation
            .codegen_representation_type(RepresentationRole::ScalarU64)
            .expect("incorrect numeric width representation");

        for (role, record) in [
            (RuntimeAbiRole::RealPower, "RealBits"),
            (RuntimeAbiRole::ComplexMultiply, "ComplexBits"),
            (RuntimeAbiRole::ComplexDivide, "ComplexBits"),
            (RuntimeAbiRole::ComplexPower, "ComplexBits"),
        ] {
            let definition = symbols
                .structures()
                .iter()
                .find(|symbol| {
                    symbols
                        .member_name(symbol.id().into())
                        .is_some_and(|name| name.as_str() == record)
                })
                .expect("production numeric record");

            let record = crate::compilation::substitution::named_type(
                values,
                bray_symbols::NamedTypeSymbolId::Struct(definition.id()),
            )
            .expect("numeric record type");

            let pointer = compilation
                .available_compiler_known_symbols()
                .unary_representation_type(values, RepresentationRole::RawPointer, record)
                .expect("numeric pointer representation")
                .expect("numeric record has a pointer type");

            for (parameters, expected) in [
                ([width, pointer, pointer, pointer], true),
                ([wrong_width, pointer, pointer, pointer], false),
                ([width, record, pointer, pointer], false),
            ] {
                let signature = CodegenCallableSignature::new(
                    parameters.map(|ty| CodegenParameterMapping::direct(ty, None, [])),
                    CodegenResultMapping::Void,
                    CallableAbi::C,
                    false,
                );

                assert_eq!(
                    runtime_source_signature_matches(
                        &compilation,
                        role,
                        &signature,
                        &CancellationToken::new(),
                        &mut DiagnosticBag::new(),
                    )
                    .expect("numeric signature must be checked"),
                    Some(expected),
                    "{target:?} {role:?}",
                );
            }
        }
    }
}
