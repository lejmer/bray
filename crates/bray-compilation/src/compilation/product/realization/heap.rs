use bray_binder::BindingQueryContext;
use bray_codegen::{CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_ir::{MirUnit, MirUnitId};
use bray_lowering::HeapStorageMethod;
use bray_symbols::{
    CallableDefinitionId, GenericArgument, GenericParameterSymbolId,
    TraitCallableFulfillmentSymbolId, TraitSymbolId,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn compiler_provided_heap_method(
        &self,
        definition: CallableDefinitionId,
    ) -> Option<HeapStorageMethod> {
        for (name, method) in [
            ("HeapStorageCreate", HeapStorageMethod::Create),
            ("HeapStorageBorrow", HeapStorageMethod::Borrow),
            ("HeapStorageBorrowMut", HeapStorageMethod::BorrowMut),
            ("HeapStorageRelease", HeapStorageMethod::Release),
        ] {
            let key = CompilerKnownDeclarationKey::try_new(name)
                .expect("heap storage operation has a valid compiler-known identity");

            if self
                .available_compiler_known_symbols()
                .declaration_symbol::<TraitCallableFulfillmentSymbolId>(&key)
                .is_some_and(|symbol| definition.callable_symbol() == symbol.into())
            {
                return Some(method);
            }
        }

        None
    }

    pub(in crate::compilation::product) fn codegen_heap_method_mir(
        &self,
        instance: &ConcreteCodegenInstance,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let definition = self.codegen_callable_definition(instance.key())?;

        let missing = |data| {
            ProductQueryFailure::missing(ProductQueryContext::CallableDefinition(definition), data)
        };

        let method = self
            .compiler_provided_heap_method(definition)
            .ok_or_else(|| missing(ProductDataKind::CallableFulfillment))?;

        let callable = instance
            .callable_instance()
            .ok_or_else(|| missing(ProductDataKind::CallableInstance))?;

        let values = self.semantic_value_store()?;

        let substitution = values.generic_substitution_data(callable.substitution());

        let binding = self.binding_context(cancellation)?;

        let storage_key = CompilerKnownDeclarationKey::try_new("Storage")
            .expect("Storage has a valid compiler-known identity");

        let storage_definition = self
            .available_compiler_known_symbols()
            .declaration_symbol::<TraitSymbolId>(&storage_key)
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CompilerKnownDeclaration(storage_key),
                    ProductDataKind::Symbol,
                )
            })?;

        let storage_trait = binding
            .symbols()
            .trait_symbol(storage_definition)
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol(storage_definition.into()),
                    ProductDataKind::Symbol,
                )
            })?;

        let [parameter] = storage_trait.generic_type_parameters() else {
            return Err(ProductQueryFailure::count_mismatch(
                ProductQueryContext::Symbol(storage_definition.into()),
                ProductDataKind::GenericSubstitution,
                1,
                storage_trait.generic_type_parameters().len(),
            )
            .into());
        };

        let argument = substitution
            .argument_for(GenericParameterSymbolId::Type(*parameter))
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol((*parameter).into()),
                    ProductDataKind::GenericSubstitution,
                )
            })?;

        let GenericArgument::Type(element) = argument else {
            panic!(
                "product UnexpectedKind construction failed: context={:?}, expected={:?}, actual={:?}",
                ProductQueryContext::Symbol((*parameter).into()),
                crate::compilation::ProductValueKind::GenericTypeArgument,
                crate::compilation::ProductValueKind::ConstantArgument
            );
        };

        let signature = self.codegen_instance_signature(instance, cancellation)?;

        let [
            CodegenParameterMapping::Direct {
                ty: parameter_type, ..
            },
        ] = signature.parameters()
        else {
            panic!(
                "product Conflict construction failed: context={:?}, data={:?}",
                ProductQueryContext::CallableData(callable),
                ProductDataKind::CallableParameters
            );
        };

        let result = match signature.result() {
            CodegenResultMapping::Direct { ty, .. } => Some(*ty),
            CodegenResultMapping::Void => None,
            CodegenResultMapping::Indirect { .. } => {
                panic!(
                    "product Conflict construction failed: context={:?}, data={:?}",
                    ProductQueryContext::CallableData(callable),
                    ProductDataKind::ResultRepresentation
                );
            }
        };

        let context =
            super::synthetic::CompilationSyntheticLoweringContext::new(self, None, cancellation)?;

        bray_lowering::lower_heap_storage(
            &context,
            bray_lowering::HeapStorageLoweringInput::new(
                unit,
                definition,
                method,
                element,
                *parameter_type,
                result,
                instance.key().target().clone(),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
    use bray_ir::{
        MirBlockKind, MirCleanupPhase, MirOperationKind, MirTerminatorKind, MirUnitId, MirUnitKey,
    };
    use bray_symbols::{NamedTypeSymbolId, PackageIdentity, StructSymbolId};

    use crate::compilation::operation::selected_storage_callable;
    use crate::compilation::substitution::named_type;
    use crate::fact::CancellationToken;
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    #[test]
    fn box_construction_infers_contained_types_and_respects_explicit_policy() {
        for (body, valid) in [
            ("func make() { let value = box(42); }", true),
            ("func make() { let value = box[Heap](42); }", true),
            (
                "func make() { let value: box Item = box({ value = 42 }); }",
                true,
            ),
            ("func make() -> box i32 { return box(42); }", true),
            ("func make() -> box i32 { return box[Heap](42); }", true),
            (
                "func make() -> box Item { return box({ value = 42 }); }",
                true,
            ),
            ("func make() -> box box i32 { return box(box(42)); }", true),
            ("func make() -> box i32 { return box[Item](42); }", false),
            ("func make() { let value = box[Item](42); }", false),
            ("func make() { let value = box[](42); }", false),
            ("func make() { let value = box[Heap, Heap](42); }", false),
            ("func make() { let value = box[Heap][Heap](42); }", false),
            ("func make() { let value = box[42](42); }", false),
            ("func make() -> box i32 { return box(true); }", false),
            (
                "func make() -> box i32 { return box(42, unknown = 7); }",
                false,
            ),
        ] {
            let source = format!("module app; struct Item {{ value: i32; }} {body}");
            let compilation = crate::test_support::compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_ne!(diagnostics.has_errors(), valid, "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn custom_box_construction_checks_policy_arguments_and_defaults() {
        for (parameter, arguments, valid) in [
            ("storage: i32", "42, storage = 1", true),
            ("storage: i32 = 7", "42", true),
            ("storage: i32 = 7", "42, storage = 1", true),
            ("pos storage: i32", "42, 1", true),
            ("pos storage: i32", "42, storage = 1", true),
            ("storage: i32?", "42, storage = supplied()", true),
            ("storage: i32? = none", "42", true),
            ("storage: i32", "42", false),
            ("storage: i32", "42, 1", false),
            ("storage: i32", "42, unknown = 1", false),
            ("storage: i32", "42, storage = 1, storage = 2", false),
            ("storage: i32", "42, storage = true", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Policy {{ mut value: i32; }}

                impl Policy(Storage<i32>)
                {{
                    trusted static func create(pos value: i32, {parameter}) -> Self {{ loop {{}} }}

                    static func borrow(pos storage: &Self) -> &i32 executes(pure, total)
                    {{
                        return &storage.value;
                    }}

                    static func borrow_mut(pos storage: &mut Self) -> &mut i32 executes(pure, total)
                    {{
                        return &mut storage.value;
                    }}

                    trusted static func release(pos storage: Self) {{ loop {{}} }}
                }}

                func supplied() -> i32 {{ return 7; }}
                func make() {{ let value = box[Policy]({arguments}); }}
                "#
            );

            let compilation = crate::test_support::compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                assert!(
                    diagnostics.iter().any(|diagnostic| {
                        diagnostic.kind()
                            == bray_diagnostics::DiagnosticKind::CheckingIncompatibleCandidate
                            && diagnostic.primary_span().is_some()
                    }),
                    "{source}: {diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn heap_policy_cannot_be_constructed_from_source_fields() {
        let compilation = crate::test_support::compilation(
            "module app; func forged() -> Heap { return Heap {}; }",
        );

        assert!(
            compilation.check_diagnostics().has_errors(),
            "source must not manufacture an uninitialized heap handle"
        );
    }

    #[test]
    fn malformed_box_policies_report_the_same_source_diagnostic_in_types_and_expressions() {
        for policy in ["[]", "[Heap, Heap]", "[42]"] {
            for body in [
                format!("func make() {{ let value = box{policy}(42); }}"),
                format!("func make(pos value: box{policy} i32) {{}}"),
            ] {
                let compilation = crate::test_support::compilation(&format!("module app; {body}"));
                let diagnostics = compilation.check_diagnostics();

                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::BindingInvalidBoxStoragePolicy,
                );

                let diagnostic = diagnostics
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.kind()
                            == bray_diagnostics::DiagnosticKind::BindingInvalidBoxStoragePolicy
                    })
                    .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));

                assert!(diagnostic.primary_span().is_some());

                let rendered = bray_messages::DiagnosticRenderer::english().render(diagnostic);

                assert_eq!(
                    rendered.labels()[0].message(),
                    "this box storage policy must contain exactly one type"
                );

                assert!(rendered.message().contains("box storage policy"));
                assert!(rendered.message().contains(policy));
                assert!(rendered.message().contains("exactly one type"));
                assert!(!rendered.message().contains("internal compiler"));
            }
        }
    }

    #[test]
    fn heap_methods_have_valid_declaration_owned_bodies_and_failed_creation_cleans_input() {
        let request = CompilationRequest::new(
            PackageIdentity::try_new("std").expect("fixture package identity must be valid"),
            vec![source_input(
                r#"trusted internal module std.runtime.memory;

using core.memory;
trusted func allocate(pos bytes: usize, pos align: usize) -> DataPointer<u8>
{
    return core.memory.null<u8>();
}
trusted func deallocate(pos pointer: DataPointer<u8>, pos bytes: usize, pos align: usize)
{
}
"#,
                0,
            )],
        )
        .with_standard_library_source_authority();

        let compilation = Compilation::load(request).expect("fixture input must load");
        let cancellation = CancellationToken::new();

        let binding = compilation
            .binding_context(&cancellation)
            .expect("fixture compilation must provide a binding context");

        let values = compilation
            .semantic_value_store()
            .expect("fixture compilation must provide its semantic value store");

        let heap = compilation
            .available_compiler_known_symbols()
            .declaration_symbol::<StructSymbolId>(
                &CompilerKnownDeclarationKey::try_new("Heap")
                    .expect("fixture compiler known declaration key must be valid"),
            )
            .expect("fixture `declaration_symbol` call must succeed");

        let heap = named_type(values, NamedTypeSymbolId::Struct(heap))
            .expect("fixture `named_type` call must succeed");

        let element = compilation
            .codegen_representation_type(RepresentationRole::ScalarI32)
            .expect("fixture type must have a code-generation representation");

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("fixture target must support code generation");

        for member in [
            "StorageCreate",
            "StorageBorrow",
            "StorageBorrowMut",
            "StorageRelease",
        ] {
            let selected = selected_storage_callable(
                &compilation,
                &binding,
                heap,
                element,
                &CompilerKnownDeclarationKey::try_new(member)
                    .expect("fixture compiler known declaration key must be valid"),
                &cancellation,
            )
            .expect("fixture `selected_storage_callable` call must succeed");

            assert!(
                !selected.diagnostics().has_errors(),
                "{:#?}",
                selected.diagnostics()
            );

            let (_, _, callable, _) = selected
                .value()
                .as_ref()
                .expect("fixture optional value must be present");

            let instance = compilation
                .concrete_codegen_callable(*callable, [], &target, &cancellation)
                .expect("fixture `concrete_codegen_callable` call must succeed");

            assert_eq!(
                instance.key().template(),
                &MirUnitKey::CompilerProvidedCallable(callable.definition())
            );

            let mir = compilation
                .codegen_heap_method_mir(&instance, MirUnitId::new(0), &cancellation)
                .unwrap_or_else(|error| {
                    panic!(
                        "{member}: {error:?}, substitution: {:?}, signature: {:?}",
                        values.generic_substitution_data(callable.substitution()),
                        compilation.codegen_instance_signature(&instance, &cancellation)
                    )
                });

            assert_eq!(mir.key(), instance.key().template());

            for block in mir.blocks() {
                if block.operations().iter().any(|id| {
                    matches!(
                        mir.operation(*id).map(|operation| operation.kind()),
                        Some(
                            MirOperationKind::Destroy { .. }
                                | MirOperationKind::Cleanup {
                                    collection: None,
                                    ..
                                }
                        )
                    )
                }) {
                    assert!(
                        matches!(
                            block.terminator().kind(),
                            MirTerminatorKind::CheckCallOutcome { .. }
                        ),
                        "{member} must check every cleanup operation before continuing: {block:?}"
                    );
                }
            }

            if member == "StorageCreate" {
                assert!(mir.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Cleanup {
                        collection: Some(_),
                        ..
                    }
                )));

                assert!(mir.blocks().iter().any(|block| matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::CheckCallOutcome { .. }
                )));

                assert!(mir.operations().iter().any(
                    |operation| matches!(operation.kind(), MirOperationKind::Cleanup {
                    phase: MirCleanupPhase::LifecycleResolution, place, .. } if place.ty() == element)
                ));

                assert!(
                    mir.blocks()
                        .iter()
                        .any(|block| block.kind() == MirBlockKind::CleanupBroadcast)
                );
            }
        }
    }
}
