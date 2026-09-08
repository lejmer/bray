use bray_binder::BindingQueryContext;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableInstanceData, CallableSignature, SelfTypeContext, TypeAssociatedLifecycleSlot,
    TypeAssociatedMemberOrigin, TypeData, TypeId,
};

use super::super::binder::CompilationBindingContext;
use super::signature::{
    associated_member_substitution, selected_callable_signature, substitute_callable_self,
};
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn selected_lifecycle_callable(
    binding: &CompilationBindingContext<'_>,
    ty: TypeId,
    slot: TypeAssociatedLifecycleSlot,
) -> Result<DiagnosticResult<Option<(CallableInstanceData, CallableSignature)>>, FactQueryError> {
    let values = binding.semantic_values();
    let data = values.type_data(ty)?;

    let TypeData::Named {
        definition,
        substitution,
    } = data.as_ref()
    else {
        return Ok(DiagnosticResult::without_diagnostics(None));
    };

    let surface = binding
        .compilation()
        .type_associated_surface_result_with_cancellation(*definition, binding.cancellation())?;

    let mut diagnostics = DiagnosticBag::new().merged(surface.diagnostics());

    if diagnostics.has_errors() {
        return Ok(DiagnosticResult::new(None, diagnostics));
    }

    let mut members = surface
        .value()
        .lifecycle_members()
        .iter()
        .filter(|member| member.slot() == slot);

    let Some(member) = members.next() else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    if members.next().is_some() {
        return Err(SemanticQueryFailure::contract(
            SemanticQueryContext::Type(ty),
            SemanticQueryViolation::CountMismatch {
                data: SemanticDataKind::OperationSelection,
                expected: 1,
                actual: surface
                    .value()
                    .lifecycle_members()
                    .iter()
                    .filter(|member| member.slot() == slot)
                    .count(),
            },
        )
        .into());
    }

    let Some(substitution) = associated_member_substitution(
        binding,
        surface.value(),
        member.origin(),
        ty,
        *substitution,
        &mut diagnostics,
    )?
    else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let instance =
        super::super::implementation::callable_instance(values, member.id(), [substitution])?;

    let signature = selected_callable_signature(binding, instance)?;

    diagnostics = diagnostics.merged(signature.diagnostics());

    let (signature, _) = signature.into_parts();

    let signature = signature.ok_or_else(|| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Symbol(member.id()),
            SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature),
        )
    })?;

    let context = match member.origin() {
        TypeAssociatedMemberOrigin::Direct => SelfTypeContext::NamedType(*definition),
        TypeAssociatedMemberOrigin::InherentImplementation(implementation) => {
            SelfTypeContext::Implementation(implementation.into())
        }
    };

    let signature = substitute_callable_self(binding, signature, context, ty)?;

    Ok(DiagnosticResult::new(
        Some((instance, signature)),
        diagnostics,
    ))
}

#[cfg(test)]
mod tests {
    use super::selected_lifecycle_callable;
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;
    use bray_symbols::{SymbolOrigin, TypeAssociatedLifecycleSlot};

    #[test]
    fn lifecycle_selection_preserves_direct_and_inherent_generic_self() {
        for source in [
            "module app; struct Resource { finalize() {} }",
            "module app; struct Resource {} impl Resource { finalize() {} }",
            "module app; struct Resource<T> { finalize() {} }",
            "module app; struct Resource<T> {} impl Resource<U> { finalize() {} }",
        ] {
            let compilation = compilation(source);
            let cancellation = CancellationToken::new();
            let binding = compilation.binding_context(&cancellation).unwrap();
            let symbols = compilation.symbol_graph().unwrap();

            let definition = symbols
                .structures()
                .iter()
                .find(|symbol| symbol.origin() == SymbolOrigin::Source)
                .unwrap()
                .id();

            let values = compilation.semantic_value_store().unwrap();

            let ty = values
                .intern_open_named_type(symbols, definition.into())
                .unwrap()
                .unwrap();

            let selected =
                selected_lifecycle_callable(&binding, ty, TypeAssociatedLifecycleSlot::Finalizer)
                    .unwrap();

            assert!(
                !selected.diagnostics().has_errors(),
                "{source}: {:?}",
                selected.diagnostics()
            );

            let (_, signature) = selected.value().as_ref().unwrap();

            assert_eq!(
                values
                    .unborrowed_type(signature.receiver().unwrap().ty())
                    .unwrap(),
                ty,
                "{source}"
            );

            assert!(
                selected_lifecycle_callable(&binding, ty, TypeAssociatedLifecycleSlot::Destructor)
                    .unwrap()
                    .value()
                    .is_none()
            );
        }
    }

    #[test]
    fn invalid_lifecycle_slots_preserve_selection_diagnostics() {
        let compilation =
            compilation("module app; struct Resource { finalize() {} finalize() {} }");

        let cancellation = CancellationToken::new();
        let binding = compilation.binding_context(&cancellation).unwrap();
        let symbols = compilation.symbol_graph().unwrap();

        let definition = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap()
            .id();

        let ty = compilation
            .semantic_value_store()
            .unwrap()
            .intern_open_named_type(symbols, definition.into())
            .unwrap()
            .unwrap();

        let selected =
            selected_lifecycle_callable(&binding, ty, TypeAssociatedLifecycleSlot::Finalizer)
                .unwrap();

        assert!(selected.diagnostics().has_errors());
        assert!(selected.value().is_none());
    }
}
