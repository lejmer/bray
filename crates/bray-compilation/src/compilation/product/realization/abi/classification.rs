use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_codegen::{
    CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping, CodegenTarget,
    CodegenTypeMapping, CodegenValueAttribute,
};
use bray_symbols::{BorrowKind, CallableAbi, TypeId};

use super::super::support::{indirect_abi_value, indirect_parameter_kind};
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
                return Err(CodegenPreparationError::InvalidAbiMapping);
            }
        };

        let mut registers = sysv::applies(signature.abi(), target)
            .then(|| sysv::Registers::new(matches!(result, CodegenResultMapping::Indirect { .. })));

        let mut parameters = Vec::with_capacity(signature.parameters().len());

        for parameter in signature.parameters() {
            let ty = match parameter {
                CodegenParameterMapping::Direct { ty, .. } => *ty,
                CodegenParameterMapping::Ignore | CodegenParameterMapping::Indirect { .. } => {
                    return Err(CodegenPreparationError::InvalidAbiMapping);
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
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenParameterMapping::Ignore);
        }

        if sysv::applies(abi, target) && sysv::is_aggregate(mapping.kind()) {
            if let Some(coercion) = sysv::coercion(ty, mappings)? {
                return Ok(CodegenParameterMapping::coerced(ty, coercion));
            }
        } else if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenParameterMapping::direct(ty, None, []));
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
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenResultMapping::Void);
        }

        if sysv::applies(abi, target) && sysv::is_aggregate(mapping.kind()) {
            if let Some(coercion) = sysv::coercion(ty, mappings)? {
                return Ok(CodegenResultMapping::coerced(ty, coercion));
            }
        } else if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenResultMapping::direct(ty, None, []));
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
    fn aggregate_register_exhaustion_rolls_back_for_later_parameters_and_counts_sret() {
        let compilation = compilation("module app;");

        let (scalar, pair) = pair_types(&compilation);

        let values = compilation.semantic_value_store().unwrap();
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
            .unwrap();

        let mut mappings = BTreeMap::from([
            (
                scalar,
                CodegenTypeMapping::new(
                    scalar,
                    layout(8, 8),
                    CodegenTypeKind::UnsignedInteger(NonZeroU16::new(64).unwrap()),
                ),
            ),
            (
                byte,
                CodegenTypeMapping::new(
                    byte,
                    layout(1, 1),
                    CodegenTypeKind::UnsignedInteger(NonZeroU16::new(8).unwrap()),
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

        assert!(sysv::coercion(pair, &mappings).unwrap().is_none());

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
            .unwrap();

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
        let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap();

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();

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

        let compilation = Compilation::load_with_codegen(request, codegen).unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let values = compilation.semantic_value_store().unwrap();
        let symbols = compilation.symbol_graph().unwrap();

        let types = symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| named_type(values, NamedTypeSymbolId::Struct(symbol.id())).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(types.len(), 3);

        let mut mappings = compilation
            .codegen_types(
                types.iter().copied().collect(),
                None,
                &target,
                &CancellationToken::new(),
            )
            .unwrap()
            .into_iter()
            .map(|mapping| (mapping.ty(), mapping))
            .collect::<BTreeMap<_, _>>();

        let opaque = types
            .iter()
            .copied()
            .find(|ty| mappings[ty].layout().unwrap().size() == 8)
            .unwrap();

        for ty in types {
            let large = mappings[&ty].layout().unwrap().size() > 16;

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
                    parameter.unwrap(),
                    CodegenParameterMapping::Indirect {
                        kind: CodegenIndirectParameterKind::ByValue,
                        ..
                    }
                ));

                assert!(matches!(
                    result.unwrap(),
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
            .unwrap();

        let pair = intern_type(
            compilation.semantic_value_store().unwrap(),
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
            .unwrap()
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
            .unwrap()
    }

    fn integer(bits: u16) -> CodegenAbiScalar {
        CodegenAbiScalar::Integer(NonZeroU16::new(bits).unwrap())
    }

    fn layout(size: u64, alignment: u64) -> TargetValueLayout {
        TargetValueLayout::new(
            size,
            NonZeroU64::new(alignment).unwrap(),
            TargetLayoutContract::C,
        )
    }
}
