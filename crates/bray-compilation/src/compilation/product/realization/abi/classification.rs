use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_codegen::{
    CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping, CodegenTarget,
    CodegenTypeMapping, CodegenValueAttribute,
};
use bray_symbols::{BorrowKind, CallableAbi, TypeId};

use super::super::support::{
    direct_abi_coercion, indirect_abi_value, indirect_parameter_kind, is_aggregate, windows_x64_abi,
};
use super::sysv;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product::realization) fn classify_codegen_signature(
        &self,
        signature: CodegenCallableSignature,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let result = match signature.result() {
            CodegenResultMapping::Void => CodegenResultMapping::Void,
            CodegenResultMapping::Direct { ty, .. } => self.classify_codegen_result(
                *ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?,
            CodegenResultMapping::Indirect { .. } => {
                panic!(
                    "ABI classification requires a semantic signature, got {signature:?} for {target:?}"
                );
            }
        };

        let mut registers = sysv::applies(signature.abi(), target)
            .then(|| sysv::Registers::new(matches!(result, CodegenResultMapping::Indirect { .. })));

        let mut parameters = Vec::with_capacity(signature.parameters().len());

        for parameter in signature.parameters() {
            let ty = match parameter {
                CodegenParameterMapping::Direct { ty, .. } => *ty,
                CodegenParameterMapping::Ignore | CodegenParameterMapping::Indirect { .. } => {
                    panic!(
                        "ABI classification requires a semantic signature, got {signature:?} for {target:?}"
                    );
                }
            };

            let mut parameter = self.classify_codegen_parameter(
                ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?;

            if let Some(registers) = registers.as_mut()
                && !registers.consume(&parameter, mappings)
                && matches!(
                    parameter,
                    CodegenParameterMapping::Direct {
                        coercion: Some(_),
                        ..
                    }
                )
            {
                // An aggregate uses registers only when every one of its pieces fits.
                parameter = self.indirect_codegen_parameter(
                    ty,
                    signature.abi(),
                    target,
                    cancellation,
                    mappings,
                    pending,
                )?;
            }

            parameters.push(parameter);
        }

        let classified = CodegenCallableSignature::new(
            parameters,
            result,
            signature.abi(),
            signature.is_variadic(),
        );

        if signature.has_panic_report_context() {
            Ok(classified.with_panic_report_context())
        } else {
            Ok(classified)
        }
    }

    pub(in crate::compilation::product::realization) fn classify_codegen_parameter(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenParameterMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .unwrap_or_else(|| panic!("checked native UnresolvedType contract failed: {:?}", ty));

        let layout = mapping.layout().unwrap_or_else(|| {
            panic!(
                "checked native UnsizedTypeByValue contract failed: {:?}",
                ty
            )
        });

        if layout.size() == 0 {
            return Ok(CodegenParameterMapping::Ignore);
        }

        if sysv::applies(abi, target) && is_aggregate(mapping.kind()) {
            if let Some(coercion) = sysv::coercion(ty, mappings)? {
                return Ok(CodegenParameterMapping::coerced(ty, coercion));
            }
        } else if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(
                match direct_abi_coercion(abi, mapping.kind(), layout, target) {
                    Some(coercion) => CodegenParameterMapping::coerced(ty, coercion),
                    None => CodegenParameterMapping::direct(ty, None, []),
                },
            );
        }

        self.indirect_codegen_parameter(ty, abi, target, cancellation, mappings, pending)
    }

    fn indirect_codegen_parameter(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenParameterMapping, CodegenPreparationError> {
        let layout = mappings[&ty]
            .layout()
            .expect("classified parameter has a sized layout");

        let alignment = if sysv::applies(abi, target) {
            layout
                .alignment()
                .max(NonZeroU64::new(8).expect("SysV stack alignment"))
        } else if windows_x64_abi(abi, target) {
            layout
                .alignment()
                .max(NonZeroU64::new(16).expect("Win64 aggregate temporary alignment"))
        } else {
            layout.alignment()
        };

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Shared)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenParameterMapping::indirect(
            pointer,
            ty,
            indirect_parameter_kind(abi, target),
            alignment,
            [CodegenValueAttribute::NonNull],
        ))
    }

    pub(in crate::compilation::product::realization) fn classify_codegen_result(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenResultMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .unwrap_or_else(|| panic!("checked native UnresolvedType contract failed: {:?}", ty));

        let layout = mapping.layout().unwrap_or_else(|| {
            panic!(
                "checked native UnsizedTypeByValue contract failed: {:?}",
                ty
            )
        });

        if layout.size() == 0 {
            return Ok(CodegenResultMapping::Void);
        }

        if sysv::applies(abi, target) && is_aggregate(mapping.kind()) {
            if let Some(coercion) = sysv::coercion(ty, mappings)? {
                return Ok(CodegenResultMapping::coerced(ty, coercion));
            }
        } else if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(
                match direct_abi_coercion(abi, mapping.kind(), layout, target) {
                    Some(coercion) => CodegenResultMapping::coerced(ty, coercion),
                    None => CodegenResultMapping::direct(ty, None, []),
                },
            );
        }

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Mutable)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenResultMapping::indirect(
            pointer,
            ty,
            layout.alignment(),
            [
                CodegenValueAttribute::NoAlias,
                CodegenValueAttribute::NonNull,
            ],
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::{NonZeroU16, NonZeroU64};
    use std::sync::Arc;

    use bray_codegen::{
        CodeGenerator, CodeGeneratorRegistry, CodegenAbiScalar, CodegenCallableSignature,
        CodegenConfiguration, CodegenFieldLayout, CodegenIndirectParameterKind,
        CodegenParameterMapping, CodegenResultMapping, CodegenTarget, CodegenTypeKind,
        CodegenTypeMapping,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::testing::intern_type;
    use bray_symbols::{
        CallableAbi, NamedTypeSymbolId, ProductKind, SymbolOrigin, TypeData, TypeId,
    };
    use bray_target::{NativeTarget, TargetLayoutContract, TargetValueLayout};

    use super::sysv;
    use crate::compilation::CodegenPreparationError;
    use crate::compilation::substitution::named_type;
    use crate::test_support::{
        compilation, package_identity, runtime_standard_library_dependency, source_input,
    };
    use crate::{
        CancellationToken, Compilation, CompilationOptions, CompilationRequest, SelectedTarget,
        WorkerBudget,
    };

    #[test]
    fn foreign_aggregate_coercion_preserves_other_target_and_bray_contracts() {
        let compilation = compilation("module app;");

        let (scalar, pair) = pair_types(&compilation);

        for native in [
            NativeTarget::X86_64LinuxGnu,
            NativeTarget::X86_64MacOs,
            NativeTarget::X86_64WindowsMsvc,
            NativeTarget::Aarch64LinuxGnu,
            NativeTarget::Aarch64MacOs,
            NativeTarget::Aarch64WindowsMsvc,
        ] {
            for abi in [CallableAbi::C, CallableAbi::System, CallableAbi::Bray] {
                let target = CodegenTarget::for_native(native);
                let signature = classify(&compilation, &target, abi, [pair], pair);

                let sysv = abi != CallableAbi::Bray
                    && matches!(
                        native,
                        NativeTarget::X86_64LinuxGnu | NativeTarget::X86_64MacOs
                    );

                match &signature.parameters()[0] {
                    CodegenParameterMapping::Direct { coercion, .. } => {
                        assert_eq!(coercion.is_some(), sysv, "{native:?} {abi:?}");

                        if let Some(coercion) = coercion {
                            assert_eq!(
                                coercion
                                    .pieces()
                                    .iter()
                                    .map(|piece| (piece.offset_bytes, piece.scalar))
                                    .collect::<Vec<_>>(),
                                [(0, integer(64)), (8, integer(64))]
                            );
                        }
                    }
                    CodegenParameterMapping::Indirect { kind, .. } => {
                        assert_eq!(native, NativeTarget::X86_64WindowsMsvc);
                        assert_ne!(abi, CallableAbi::Bray);
                        assert_eq!(*kind, CodegenIndirectParameterKind::Reference);
                    }
                    CodegenParameterMapping::Ignore => panic!("nonempty pair must be passed"),
                }

                match signature.result() {
                    CodegenResultMapping::Direct { coercion, .. } => {
                        assert_eq!(coercion.is_some(), sysv)
                    }
                    CodegenResultMapping::Indirect { .. } => {
                        assert_eq!(native, NativeTarget::X86_64WindowsMsvc)
                    }
                    CodegenResultMapping::Void => panic!("nonempty pair must be returned"),
                }

                let scalar_signature = classify(&compilation, &target, abi, [scalar], scalar);

                assert!(matches!(
                    scalar_signature.parameters(),
                    [CodegenParameterMapping::Direct { coercion: None, .. }]
                ));
            }
        }
    }

    #[test]
    fn windows_aggregates_use_one_integer_register_or_an_aligned_temporary() {
        let compilation = compilation("module app;");
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let values = compilation
            .semantic_value_store()
            .expect("fixture compilation must provide its semantic value store");

        let byte = compilation
            .compiler_known_type(RepresentationRole::ScalarU8)
            .expect("fixture compiler-known type must resolve");

        for bytes in [1, 2, 3, 4, 5, 8, 16] {
            let aggregate = intern_type(values, TypeData::tuple(std::iter::repeat_n(byte, bytes)));

            for abi in [CallableAbi::C, CallableAbi::System] {
                let signature = classify(&compilation, &target, abi, [aggregate], aggregate);

                if matches!(bytes, 1 | 2 | 4 | 8) {
                    let expected = [(
                        0,
                        integer(
                            u16::try_from(bytes * 8)
                                .expect("fixture value must fit the requested representation"),
                        ),
                    )];

                    let CodegenParameterMapping::Direct {
                        coercion: Some(parameter),
                        ..
                    } = &signature.parameters()[0]
                    else {
                        panic!("small aggregate uses a register");
                    };

                    let CodegenResultMapping::Direct {
                        coercion: Some(result),
                        ..
                    } = signature.result()
                    else {
                        panic!("small result uses a register");
                    };

                    for coercion in [parameter, result] {
                        assert_eq!(
                            coercion
                                .pieces()
                                .iter()
                                .map(|piece| (piece.offset_bytes, piece.scalar))
                                .collect::<Vec<_>>(),
                            expected
                        );
                    }
                } else {
                    assert!(
                        matches!(signature.parameters(), [CodegenParameterMapping::Indirect {
                        alignment, kind: CodegenIndirectParameterKind::Reference, ..
                    }] if alignment.get() == 16)
                    );

                    assert!(matches!(
                        signature.result(),
                        CodegenResultMapping::Indirect { .. }
                    ));
                }
            }

            let bray = classify(
                &compilation,
                &target,
                CallableAbi::Bray,
                [aggregate],
                aggregate,
            );

            assert!(matches!(
                bray.parameters(),
                [CodegenParameterMapping::Direct { coercion: None, .. }]
            ));

            assert!(matches!(
                bray.result(),
                CodegenResultMapping::Direct { coercion: None, .. }
            ));
        }

        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarR32)
            .expect("fixture compiler-known type must resolve");

        let pair = intern_type(values, TypeData::tuple([scalar, scalar]));
        let floating = classify(&compilation, &target, CallableAbi::C, [pair], pair);

        let CodegenResultMapping::Direct {
            coercion: Some(coercion),
            ..
        } = floating.result()
        else {
            panic!("Win64 floating aggregate also uses an integer register");
        };

        assert_eq!(coercion.pieces()[0].scalar, integer(64));
    }

    #[test]
    fn aggregate_register_exhaustion_rolls_back_for_later_parameters_and_counts_sret() {
        let compilation = compilation("module app;");

        let (scalar, pair) = pair_types(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("fixture compilation must provide its semantic value store");

        let small = intern_type(values, TypeData::tuple([scalar]));
        let large = intern_type(values, TypeData::tuple([scalar, scalar, scalar]));
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let parameters = [scalar, scalar, scalar, scalar, scalar, pair, small];
        let direct = classify(&compilation, &target, CallableAbi::C, parameters, pair);

        assert!(matches!(
            direct.parameters()[5],
            CodegenParameterMapping::Indirect {
                kind: CodegenIndirectParameterKind::ByValue,
                ..
            }
        ));

        assert!(matches!(
            direct.parameters()[6],
            CodegenParameterMapping::Direct {
                coercion: Some(_),
                ..
            }
        ));

        let hidden = classify(&compilation, &target, CallableAbi::C, parameters, large);

        assert!(matches!(
            hidden.result(),
            CodegenResultMapping::Indirect { .. }
        ));

        assert!(matches!(
            hidden.parameters()[5],
            CodegenParameterMapping::Indirect { .. }
        ));

        assert!(matches!(
            hidden.parameters()[6],
            CodegenParameterMapping::Indirect { .. }
        ));
    }

    #[test]
    fn unaligned_aggregate_fields_require_memory_even_when_the_value_fits_registers() {
        let compilation = compilation("module app;");

        let (scalar, pair) = pair_types(&compilation);

        let byte = compilation
            .compiler_known_type(RepresentationRole::ScalarU8)
            .expect("fixture compiler-known type must resolve");

        let mut mappings = BTreeMap::from([
            (
                scalar,
                CodegenTypeMapping::new(
                    scalar,
                    layout(8, 8),
                    CodegenTypeKind::UnsignedInteger(
                        NonZeroU16::new(64).expect("fixture non zero u16 must be valid"),
                    ),
                ),
            ),
            (
                byte,
                CodegenTypeMapping::new(
                    byte,
                    layout(1, 1),
                    CodegenTypeKind::UnsignedInteger(
                        NonZeroU16::new(8).expect("fixture non zero u16 must be valid"),
                    ),
                ),
            ),
            (
                pair,
                CodegenTypeMapping::new(
                    pair,
                    layout(9, 1),
                    CodegenTypeKind::aggregate([
                        CodegenFieldLayout::new(None, byte, 0),
                        CodegenFieldLayout::new(None, scalar, 1),
                    ]),
                ),
            ),
        ]);

        assert!(
            sysv::coercion(pair, &mappings)
                .expect("fixture `sysv::coercion` call must succeed")
                .is_none()
        );

        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let signature = compilation
            .classify_codegen_signature(
                CodegenCallableSignature::new(
                    [CodegenParameterMapping::direct(pair, None, [])],
                    CodegenResultMapping::direct(pair, None, []),
                    CallableAbi::C,
                    false,
                ),
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut BTreeSet::new(),
            )
            .expect("fixture `classify_codegen_signature` call must succeed");

        assert!(
            matches!(signature.parameters(), [CodegenParameterMapping::Indirect { alignment, kind: CodegenIndirectParameterKind::ByValue, .. }] if alignment.get() == 8)
        );

        assert!(
            matches!(signature.result(), CodegenResultMapping::Indirect { alignment, .. } if alignment.get() == 1)
        );
    }

    #[test]
    fn opaque_storage_requires_known_register_classes_but_large_values_use_memory() {
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let source = r#"
            module app;
            @layout(c, size = 8, align = 8)
            struct NativeOpaquePayload;
            @layout(c)
            struct Wrapped { storage: NativeOpaquePayload; marker: u64; }
            @layout(c, size = 24, align = 8)
            struct LargeStorage;
        "#;

        let selected = SelectedTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .expect("fixture llvm code generator must be valid"),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .expect("fixture code generator registry must be valid");

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .expect("fixture codegen configuration must be valid");

        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input(source, 0)],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                selected.clone(),
            ),
        )
        .with_dependency_interfaces([runtime_standard_library_dependency(&selected)]);

        let compilation = Compilation::load_with_codegen(request, codegen)
            .expect("fixture compilation must load for code generation");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let values = compilation
            .semantic_value_store()
            .expect("fixture compilation must provide its semantic value store");

        let symbols = compilation
            .symbol_graph()
            .expect("fixture compilation must build its symbol graph");

        let types = symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| {
                named_type(values, NamedTypeSymbolId::Struct(symbol.id()))
                    .expect("fixture `named_type` call must succeed")
            })
            .collect::<Vec<_>>();

        assert_eq!(types.len(), 3);

        let mut mappings = compilation
            .codegen_types(
                types.iter().copied().collect(),
                None,
                &target,
                &CancellationToken::new(),
            )
            .expect("fixture `codegen_types` call must succeed")
            .into_iter()
            .map(|mapping| (mapping.ty(), mapping))
            .collect::<BTreeMap<_, _>>();

        let opaque = types
            .iter()
            .copied()
            .find(|ty| {
                mappings[ty]
                    .layout()
                    .expect("fixture `layout` call must succeed")
                    .size()
                    == 8
            })
            .expect("fixture lookup must find its expected value");

        for ty in types {
            let large = mappings[&ty]
                .layout()
                .expect("fixture `layout` call must succeed")
                .size()
                > 16;

            let parameter = compilation.classify_codegen_parameter(
                ty,
                CallableAbi::C,
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut BTreeSet::new(),
            );

            let result = compilation.classify_codegen_result(
                ty,
                CallableAbi::C,
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut BTreeSet::new(),
            );

            if large {
                assert!(matches!(
                    parameter.expect("fixture `parameter` call must succeed"),
                    CodegenParameterMapping::Indirect {
                        kind: CodegenIndirectParameterKind::ByValue,
                        ..
                    }
                ));

                assert!(matches!(
                    result.expect("fixture `result` call must succeed"),
                    CodegenResultMapping::Indirect { .. }
                ));
            } else {
                assert_eq!(
                    parameter,
                    Err(CodegenPreparationError::UnsupportedType(opaque))
                );

                assert_eq!(
                    result,
                    Err(CodegenPreparationError::UnsupportedType(opaque))
                );
            }
        }
    }

    fn pair_types(compilation: &Compilation) -> (TypeId, TypeId) {
        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarU64)
            .expect("fixture compiler-known type must resolve");

        let pair = intern_type(
            compilation
                .semantic_value_store()
                .expect("fixture compilation must provide its semantic value store"),
            TypeData::tuple([scalar, scalar]),
        );

        (scalar, pair)
    }

    fn classify(
        compilation: &Compilation,
        target: &CodegenTarget,
        abi: CallableAbi,
        parameters: impl IntoIterator<Item = TypeId>,
        result: TypeId,
    ) -> CodegenCallableSignature {
        let parameters = parameters.into_iter().collect::<Vec<_>>();

        let mut mappings = compilation
            .codegen_types(
                parameters.iter().copied().chain([result]).collect(),
                None,
                target,
                &CancellationToken::new(),
            )
            .expect("fixture `codegen_types` call must succeed")
            .into_iter()
            .map(|mapping| (mapping.ty(), mapping))
            .collect();

        compilation
            .classify_codegen_signature(
                CodegenCallableSignature::new(
                    parameters
                        .into_iter()
                        .map(|ty| CodegenParameterMapping::direct(ty, None, [])),
                    CodegenResultMapping::direct(result, None, []),
                    abi,
                    false,
                ),
                target,
                &CancellationToken::new(),
                &mut mappings,
                &mut BTreeSet::new(),
            )
            .expect("fixture `classify_codegen_signature` call must succeed")
    }

    fn integer(bits: u16) -> CodegenAbiScalar {
        CodegenAbiScalar::Integer(
            NonZeroU16::new(bits).expect("fixture non zero u16 must be valid"),
        )
    }

    fn layout(size: u64, alignment: u64) -> TargetValueLayout {
        TargetValueLayout::new(
            size,
            NonZeroU64::new(alignment).expect("fixture non zero u64 must be valid"),
            TargetLayoutContract::C,
        )
    }
}
