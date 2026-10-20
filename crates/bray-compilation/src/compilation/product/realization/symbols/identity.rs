use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::product::realization::names::{
    binary_symbol_name, generated_instance_symbol_name,
};
use crate::compilation::product::realization::support::source_backed_symbol_key;
use crate::compilation::product::specialization::ConcreteCodegenInstance;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;
use crate::fact::FactQueryError;
use bray_base::StableDigestHasher;
use bray_binder::BindingQueryContext;
use bray_codegen::{CodegenLinkage, CodegenTarget};
use bray_ir::MirUnitKey;
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{AnySymbolId, PackageIdentity, SymbolKey};
use std::hash::{Hash, Hasher};

impl Compilation {
    pub(in crate::compilation::product::realization) fn codegen_instance_package(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        product_package: &PackageIdentity,
        cancellation: &CancellationToken,
    ) -> Result<PackageIdentity, CodegenPreparationError> {
        let symbol = match instance.template() {
            // Compatibility metadata owns package identity beyond the semantic-key borrow.
            MirUnitKey::Bound(key) => {
                return Ok(key
                    .declared_owner()
                    .package_identity()
                    .unwrap_or(product_package)
                    .clone());
            }
            // Compatibility metadata owns package identity beyond the product-key borrow.
            MirUnitKey::ExecutableHost(product) => return Ok(product.package().clone()),
            // Generated lifecycle definitions belong to the selected product package.
            MirUnitKey::GeneratedLifecycle(_) | MirUnitKey::Predicate(_) => return Ok(product_package.clone()),
            MirUnitKey::ExternalCallable(definition)
            | MirUnitKey::CompilerProvidedCallable(definition) => definition.symbol(),
            MirUnitKey::ImportedExecutable(key) => key.owner(),
            MirUnitKey::ExternalRuntimeDefault(symbol) => *symbol,
        };

        let binding_context = self.binding_context(cancellation)?;

        let key = binding_context
            .symbol_key(symbol)
            .map_err(crate::compilation::binder::binding_query_error)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol(symbol),
                    ProductDataKind::SymbolKey,
                )
            })?;

        // Compatibility metadata owns package identity beyond the binder-binding_context borrow.
        Ok(key.package_identity().unwrap_or(product_package).clone())
    }

    pub(in crate::compilation::product::realization) fn generated_callable_symbol_name(
        &self,
        product: &bray_symbols::ProductIdentity,
        target: &CodegenTarget,
        linkage: CodegenLinkage,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<BinarySymbolName, CodegenPreparationError> {
        if let MirUnitKey::Predicate(reference) = realization.key().template() {
            return self.predicate_symbol_name(target, linkage, realization, reference, cancellation);
        }

        if let Some(provider) =
            self.codegen_runtime_default_provider(realization.key(), cancellation)?
        {
            let binding_context = self.binding_context(cancellation)?;
            let provider = self.portable_codegen_symbol_key(&binding_context, provider)?;
            let mut hasher = StableDigestHasher::new();

            hasher.write(b"bray.codegen-runtime-default-symbol");
            provider.hash(&mut hasher);

            hash_specialization(realization, &mut hasher);

            return binary_symbol_name(target, linkage, "default", hasher.finalize());
        }

        let Some(callable) = realization.callable_instance() else {
            return generated_instance_symbol_name(target, linkage, realization.key());
        };

        let binding_context = self.binding_context(cancellation)?;

        let definition =
            self.portable_codegen_symbol_key(&binding_context, callable.definition().symbol())?;

        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.codegen-callable-symbol");
        definition.hash(&mut hasher);

        if matches!(realization.key().template(), MirUnitKey::Bound(_)) {
            product.hash(&mut hasher);
        }

        hash_specialization(realization, &mut hasher);

        binary_symbol_name(target, linkage, "instance", hasher.finalize())
    }

    pub(in crate::compilation::product::realization) fn codegen_runtime_default_provider(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<AnySymbolId>, CodegenPreparationError> {
        match instance.template() {
            MirUnitKey::Bound(key)
                if key.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault =>
            {
                self.symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .map(Some)
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::SymbolKey(key.declared_owner().clone()),
                            ProductDataKind::Symbol,
                        )
                        .into()
                    })
            }
            MirUnitKey::ImportedExecutable(provider) => {
                let binding_context = self.binding_context(cancellation)?;
                let provider = provider.owner();

                Ok(binding_context
                    .runtime_default_subject(provider)
                    .map(|subject| subject.map(|_| provider))
                    .map_err(crate::compilation::binder::binding_query_error)?)
            }
            MirUnitKey::ExternalRuntimeDefault(provider) => Ok(Some(*provider)),
            MirUnitKey::Bound(_)
            | MirUnitKey::ExecutableHost(_)
            | MirUnitKey::Predicate(_)
            | MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::CompilerProvidedCallable(_)
            | MirUnitKey::ExternalCallable(_) => Ok(None),
        }
    }

    pub(in crate::compilation::product) fn portable_codegen_symbol_key(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        symbol: AnySymbolId,
    ) -> Result<SymbolKey, CodegenPreparationError> {
        let definition = binding_context
            .symbol_key(symbol)
            .map_err(crate::compilation::binder::binding_query_error)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol(symbol),
                    ProductDataKind::SymbolKey,
                )
            })?;

        if source_backed_symbol_key(definition) {
            Ok(SymbolKey::external(
                crate::compilation::export::external_symbol_key(
                    binding_context.symbols(),
                    self.package_identity(),
                    symbol,
                )
                .map_err(|cause| {
                    FactQueryError::from(ProductQueryFailure::ExternalSymbolIdentity {
                        symbol,
                        cause: Box::new(cause),
                    })
                })?,
            ))
        } else {
            // Symbol keys are Arc-backed and the generated mapping owns its identity input.
            Ok(definition.clone())
        }
    }
}

impl Compilation {
    fn predicate_symbol_name(&self, target: &CodegenTarget, linkage: CodegenLinkage,
        realization: &ConcreteCodegenInstance, reference: &bray_ir::MirPredicateReference,
        cancellation: &CancellationToken) -> Result<BinarySymbolName, CodegenPreparationError> {
        use crate::compilation::product::specialization_identity::content::{StructuralSemanticValue, structural_semantic_identity};
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;
        let mut hasher = StableDigestHasher::new();
        hasher.write(b"bray.codegen-predicate-symbol");
        match reference {
            bray_ir::MirPredicateReference::Clause { callable, ordinal } => {
                hasher.write_u8(0);
                self.portable_codegen_symbol_key(&context, callable.instance().definition().symbol())?.hash(&mut hasher);
                ordinal.hash(&mut hasher);
                callable.abi().hash(&mut hasher);
            }
            bray_ir::MirPredicateReference::Predicate(predicate) => {
                hasher.write_u8(1);
                self.portable_codegen_symbol_key(&context, predicate.definition().into_any())?.hash(&mut hasher);
            }
            bray_ir::MirPredicateReference::Condition { condition, input_types } => {
                hasher.write_u8(2);
                structural_semantic_identity(values, &context, StructuralSemanticValue::ConstantTerm(*condition))?.hash(&mut hasher);
                for ty in input_types.iter() {
                    structural_semantic_identity(values, &context, StructuralSemanticValue::Type(*ty))?.hash(&mut hasher);
                }
            }
        }
        hash_specialization(realization, &mut hasher);
        binary_symbol_name(target, linkage, "predicate", hasher.finalize())
    }
}

fn hash_specialization(realization: &ConcreteCodegenInstance, hasher: &mut StableDigestHasher) {
    realization.key().specialization().hash(hasher);
    realization.key().witnesses().hash(hasher);
    realization.key().contextual_self_witness().hash(hasher);
    realization.key().target().hash(hasher);
}
