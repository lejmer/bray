use bray_binder::BindingQueryContext;
use bray_bound_tree::DefaultValueProvider;
use bray_codegen::CodegenTarget;
use bray_ir::{
    MirExecutableTemplateId, MirHelperReference, MirImportedExecutableKey, MirOperationId,
    MirOperationKind, MirUnitKey,
};
use bray_symbols::{AnySymbolId, CallableDefinitionId, SymbolKeyData, TypeData};

use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::{CancellationToken, FactQueryError};

#[cfg(test)]
mod tests {
    #[test]
    fn construction_defaults_use_declaration_specialization_across_callers() {
        for source in [
            "module app; struct Value { digit: i32 = 0; } func first() -> Value { return Value {}; } func second() -> Value { return Value {}; } func main() { first(); second(); }",
            "module app; struct Value { digit: i32 = 0; } func first<T>(pos input: T) -> Value { return Value {}; } func second<T>(pos input: T) -> Value { return Value {}; } func main() { first<i32>(1); second<i32>(2); first<bool>(true); }",
            "module app; struct Value<T> { marker: bool; data: T? = none; } func first<U>() -> Value<U> { return { marker = true }; } func second<U>() -> Value<U> { return { marker = true }; } func main() { first<i32>(); second<i32>(); first<bool>(); }",
            "module app; struct Value<const count: i32> { marker: bool; digit: i32 = count; } func first<T>(pos input: T) -> Value<7> { return { marker = true }; } func second<T>(pos input: T) -> Value<7> { return { marker = true }; } func main() { first<i32>(1); second<i32>(2); }",
            "module app; union Value<T> { Item(data: T? = none); } func first<U>() -> Value<U> { return Item(); } func second<U>() -> Value<U> { return Item(); } func main() { first<i32>(); second<i32>(); first<bool>(); }",
        ] {
            let compilation = crate::test_support::compilation(source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let result = compilation.imported_codegen_instance_count_for_test();

            assert!(result.is_ok(), "{source}: {result:?}");
        }
    }
}

impl Compilation {
    pub(super) fn concrete_codegen_default_value(
        &self,
        owner: &ConcreteCodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        provider: DefaultValueProvider,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let reference = MirHelperReference::DefaultValue(provider);

        let MirOperationKind::Call(call) = operation else {
            return Err(ProductQueryFailure::InvalidHelperOperation {
                context: ProductQueryContext::Operation {
                    instance: owner.key().clone(),
                    operation: operation_id,
                },
                helper: reference,
                operation: operation.clone(),
            }
            .into());
        };

        let bray_ir::MirCallTarget::DefaultValue {
            owner: default_owner,
            ..
        } = call.target()
        else {
            return Err(CodegenPreparationError::MissingHelperInstance(reference));
        };

        let ty = match default_owner {
            bray_ir::MirDefaultOwner::Callable(callable) => {
                let callee = self.concrete_codegen_callable_data(
                    owner,
                    &callable.instance(),
                    target,
                    cancellation,
                )?;

                return self.concrete_codegen_runtime_default(
                    &callee,
                    provider.symbol(),
                    &reference,
                    cancellation,
                );
            }
            bray_ir::MirDefaultOwner::Type { ty, .. } => *ty,
        };

        let ty = self.concrete_codegen_type(ty, owner.substitution(), Some(owner), cancellation)?;

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty,
                expected: crate::compilation::ProductValueKind::NamedType,
                actual: data.as_ref().clone(),
            }
            .into());
        };

        // Field defaults belong to the constructed type, not the calling function's generics.
        let substitution = self.realize_codegen_substitution(*substitution)?;

        let declaration = values
            .generic_substitution_data(substitution)
            .map_err(FactQueryError::SemanticValueStore)?
            .owner();

        let requirements =
            self.concrete_codegen_constraint_requirements(declaration, substitution, cancellation)?;

        let witnesses = requirements
            .into_iter()
            .map(|requirement| {
                self.concrete_codegen_requirement_witness(
                    owner.implementation_witnesses().iter().copied(),
                    requirement,
                    cancellation,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;
        let specialization = self.codegen_specialization(substitution)?;

        let template =
            self.runtime_default_template(provider.symbol(), &reference, cancellation)?;

        Ok(ConcreteCodegenInstance::type_default(
            template,
            substitution,
            specialization,
            &witnesses,
            owner.key().target().clone(),
        ))
    }

    pub(super) fn concrete_codegen_runtime_default(
        &self,
        owner: &ConcreteCodegenInstance,
        provider: AnySymbolId,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let template = self.runtime_default_template(provider, reference, cancellation)?;

        ConcreteCodegenInstance::inherited_helper(owner, template).ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Symbol(provider),
                ProductDataKind::ConcreteInstance,
            )
            .into()
        })
    }

    fn runtime_default_template(
        &self,
        provider: AnySymbolId,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitKey, CodegenPreparationError> {
        if let Some(unit) = self.runtime_default_unit(provider)? {
            return Ok(MirUnitKey::Bound(unit));
        }

        let binding_context = self.binding_context(cancellation)?;

        let key = binding_context
            .symbol_key(provider)
            .map_err(super::super::super::binder::binding_query_error)?
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        if !matches!(key.data(), SymbolKeyData::External(_)) {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        }

        let Some(address) = binding_context
            .imported_semantic_address(provider)
            .map_err(super::super::super::binder::binding_query_error)?
        else {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        };

        let template = self.imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(address),
            cancellation,
        )?;

        if template.value().is_none() {
            return Err(CodegenPreparationError::Diagnostics(
                template.diagnostics().clone(),
            ));
        }

        Ok(MirUnitKey::ImportedExecutable(
            MirImportedExecutableKey::new(provider, MirExecutableTemplateId::ROOT),
        ))
    }

    pub(super) fn runtime_default_unit(
        &self,
        provider: AnySymbolId,
    ) -> Result<Option<bray_bound_tree::BoundUnitKey>, CodegenPreparationError> {
        Ok(self.declared_unit_key(provider, bray_bound_tree::BoundUnitKind::RuntimeDefault)?)
    }

    pub(in crate::compilation::product) fn concrete_codegen_callable_defaults(
        &self,
        callable: CallableDefinitionId,
        owner: &ConcreteCodegenInstance,
    ) -> Result<Vec<ConcreteCodegenInstance>, CodegenPreparationError> {
        let symbols = self.symbol_graph()?;

        let (parameters, _) = symbols
            .callable_parameters_and_receiver(callable.callable_symbol())
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CallableDefinition(callable),
                    ProductDataKind::CallableParameters,
                )
            })?;

        let mut defaults = Vec::new();

        for parameter in parameters {
            let Some(provider) = symbols
                .callable_parameter(*parameter)
                .and_then(bray_symbols::CallableParameterSymbol::default_provider)
            else {
                continue;
            };

            let unit = self.runtime_default_unit(provider.into())?.ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol(provider.into()),
                    ProductDataKind::RuntimeDefaultUnit,
                )
            })?;

            defaults.push(self.concrete_codegen_bound_helper(owner, unit)?);
        }

        Ok(defaults)
    }
}
