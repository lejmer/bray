use std::collections::BTreeSet;

use bray_binder::BindingQueryContext;
use bray_checker::CheckerRequestContext;
use bray_codegen::CodegenTarget;
use bray_ir::MirUnitKey;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ExactSymbolId, ImplementationSymbolId,
    ProductKind, StaticSymbolId, TraitCallableMemberSymbolId,
};

use super::super::super::Compilation;
use super::super::super::binder::has_visible_generic_parameters;
use super::super::super::implementation::implementation_fulfillments;
use super::super::super::substitution::empty_substitution;
use super::super::error::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::super::specialization::ConcreteCodegenInstance;
use super::error::NativeProductPlanningError;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product) fn product_root_instances(
        &self,
        semantic: &bray_symbols::ProductSemantics,
        test_discovery: Option<&super::super::super::testing::TestDiscovery>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ConcreteCodegenInstance>, NativeProductPlanningError> {
        let binding_context = self.binding_context(cancellation)?;

        let mut symbols = match semantic.kind() {
            ProductKind::Executable | ProductKind::Test => {
                product_entry_symbols(semantic, test_discovery)?
            }
            ProductKind::Library => semantic.public_symbols().to_vec(),
        };

        if semantic.kind() == ProductKind::Library {
            for implementation in semantic
                .public_symbols()
                .iter()
                .copied()
                .filter_map(ImplementationSymbolId::try_from_any)
            {
                symbols.extend(
                    implementation_fulfillments(&binding_context, implementation)?
                        .callables
                        .iter()
                        .copied()
                        .map(AnySymbolId::from),
                );
            }
        }

        for function in binding_context.symbols().functions() {
            if crate::compilation::foreign::has_source_role(self, function.id())? {
                symbols.push(function.id().into());

                continue;
            }

            if self
                .foreign_callable_contract_with_cancellation(function.id(), cancellation)?
                .value()
                .as_ref()
                .is_some_and(|contract| {
                    contract.direction() == bray_symbols::ForeignCallableDirection::Export
                })
            {
                symbols.push(function.id().into());
            }
        }

        for static_symbol in binding_context.symbols().statics() {
            if has_visible_generic_parameters(binding_context.symbols(), static_symbol.id().into())
            {
                continue;
            }

            let template = self.static_instance_template(static_symbol.id())?;

            let ty = self.resolve_codegen_type(
                template.value().declared_type(),
                empty_substitution(binding_context.semantic_values(), static_symbol.id().into())?,
                cancellation,
            )?;

            let native_direction =
                self.foreign_static_direction(static_symbol.id(), cancellation)?;

            if native_direction == Some(bray_symbols::ForeignCallableDirection::Import) {
                continue;
            }

            let native_export =
                native_direction == Some(bray_symbols::ForeignCallableDirection::Export);

            if native_export
                || !template.value().lifecycle_obligations().is_empty()
                || !self.codegen_cleanup_is_trivial(ty, cancellation)?
            {
                symbols.push(static_symbol.id().into());
            }
        }

        if semantic.kind() == ProductKind::Test {
            let mut seen = BTreeSet::new();

            symbols.retain(|symbol| seen.insert(*symbol));
        } else {
            symbols.sort_unstable();
            symbols.dedup();
        }

        let mut roots = Vec::new();

        for symbol in symbols {
            if TraitCallableMemberSymbolId::try_from_any(symbol).is_some() {
                continue;
            }

            if has_visible_generic_parameters(binding_context.symbols(), symbol) {
                continue;
            }

            if let Some(declaration) = StaticSymbolId::try_from_any(symbol) {
                if let Some(root) =
                    self.product_root_static(declaration, &binding_context, target, cancellation)?
                {
                    roots.push(root);
                }

                continue;
            }

            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                continue;
            };

            // Compiler-implemented declarations publish semantic operations, not native function bodies.
            if semantic.kind() == ProductKind::Library
                && self.callable_body_key(definition)?.is_none()
                && self
                    .checker_context(cancellation)?
                    .implementation_hook(symbol)
                    .map_err(FactQueryError::from)?
                    .is_some_and(|hook| hook.is_available())
            {
                continue;
            }

            let (callable, witnesses) =
                self.product_root_callable(symbol, definition, &binding_context, cancellation)?;

            let callable =
                self.concrete_codegen_callable(callable, witnesses, target, cancellation)?;

            if semantic.kind() == ProductKind::Library {
                roots.extend(self.concrete_codegen_callable_defaults(definition, &callable)?);
            }

            roots.push(callable);
        }

        if semantic.kind() != ProductKind::Test {
            roots.sort_unstable();
            roots.dedup();
        }

        if roots.is_empty() && semantic.kind() == ProductKind::Executable {
            return Err(NativeProductPlanningError::MissingProductRoot);
        }

        Ok(roots)
    }

    fn product_root_static(
        &self,
        declaration: StaticSymbolId,
        binding_context: &super::super::super::binder::CompilationBindingContext<'_>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCodegenInstance>, NativeProductPlanningError> {
        if self.foreign_static_direction(declaration, cancellation)?
            == Some(bray_symbols::ForeignCallableDirection::Import)
        {
            return Ok(None);
        }

        let initializer = self.static_initializer_key(declaration)?.ok_or_else(|| {
            FactQueryError::from(ProductQueryFailure::missing(
                ProductQueryContext::Symbol(declaration.into()),
                ProductDataKind::StaticInitializer,
            ))
        })?;

        let substitution = empty_substitution(
            binding_context.semantic_values(),
            AnySymbolId::Static(declaration),
        )?;

        let (witnesses, diagnostics) = self.static_instance_witnesses(
            declaration,
            substitution,
            cancellation,
            binding_context,
            initializer.source().syntax(),
        )?;

        if diagnostics.has_errors() {
            return Err(
                super::super::super::CodegenPreparationError::Diagnostics(diagnostics).into(),
            );
        }

        let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;
        let specialization = self.codegen_specialization(substitution)?;

        Ok(Some(ConcreteCodegenInstance::static_initializer(
            MirUnitKey::Bound(initializer),
            declaration,
            substitution,
            specialization,
            &witnesses,
            bray_ir::MirTargetContract::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        )))
    }

    fn foreign_static_direction(
        &self,
        declaration: StaticSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_symbols::ForeignCallableDirection>, FactQueryError> {
        Ok(self
            .foreign_static_contract_with_cancellation(declaration, cancellation)?
            .value()
            .as_ref()
            .map(bray_symbols::ForeignStaticContract::direction))
    }

    fn product_root_callable(
        &self,
        symbol: AnySymbolId,
        definition: CallableDefinitionId,
        binding_context: &super::super::super::binder::CompilationBindingContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            CallableInstanceData,
            Vec<bray_symbols::ImplementationInstanceId>,
        ),
        NativeProductPlanningError,
    > {
        let Some(implementation) = binding_context
            .symbols()
            .containing_symbol(symbol)
            .and_then(ImplementationSymbolId::try_from_any)
        else {
            let substitution = empty_substitution(self.semantic_value_store()?, symbol)?;

            return Ok((
                CallableInstanceData::new(definition, substitution),
                Vec::new(),
            ));
        };

        if matches!(implementation, ImplementationSymbolId::Inherent(_)) {
            let values = self.semantic_value_store()?;

            let implementation_substitution =
                empty_substitution(values, implementation.into_any())?;

            let callable = super::super::super::implementation::callable_instance(
                values,
                symbol,
                [implementation_substitution],
            )?;

            return Ok((callable, Vec::new()));
        }

        let headers = self.implementation_header_index(cancellation)?;

        let header = headers.value().header(implementation).ok_or_else(|| {
            FactQueryError::from(ProductQueryFailure::missing(
                ProductQueryContext::Symbol(implementation.into_any()),
                ProductDataKind::ImplementationHeader,
            ))
        })?;

        let values = self.semantic_value_store()?;

        let application = values.trait_application_data(header.trait_application());

        let implementation_substitution = empty_substitution(values, implementation.into_any())?;

        let callable = super::super::super::implementation::callable_instance(
            values,
            symbol,
            [application.substitution(), implementation_substitution],
        )
        .map_err(NativeProductPlanningError::from)?;

        let witness = values
            .intern_implementation_instance(bray_symbols::ImplementationInstanceData::new(
                implementation,
                implementation_substitution,
            ))
            .map_err(FactQueryError::SemanticValueStore)?;

        Ok((callable, vec![witness]))
    }
}

pub(super) fn product_entry_symbols(
    semantic: &bray_symbols::ProductSemantics,
    test_discovery: Option<&super::super::super::testing::TestDiscovery>,
) -> Result<Vec<AnySymbolId>, NativeProductPlanningError> {
    match semantic.kind() {
        ProductKind::Executable => Ok(semantic
            .entrypoint()
            .map(AnySymbolId::from)
            .into_iter()
            .collect()),
        ProductKind::Test => {
            let discovery = test_discovery.ok_or_else(|| {
                FactQueryError::from(ProductQueryFailure::missing(
                    ProductQueryContext::Product(ProductKind::Test),
                    ProductDataKind::TestDiscovery,
                ))
            })?;

            Ok(discovery
                .catalog()
                .entries()
                .iter()
                .filter_map(|entry| discovery.function(entry.identity()))
                .map(AnySymbolId::from)
                .collect())
        }
        ProductKind::Library => Ok(Vec::new()),
    }
}
