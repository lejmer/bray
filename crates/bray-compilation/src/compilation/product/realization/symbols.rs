use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_binder::BindingQueryContext;
use bray_codegen::{
    CodegenCallableMapping, CodegenConstantMapping, CodegenConstantTermMapping,
    CodegenDefinitionVisibility, CodegenInstance, CodegenLinkage, CodegenOperationMapping,
    CodegenPartitionCompatibility, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenUnit, child_constants, demanded_callable_instances,
    demanded_constant_terms, demanded_constants,
};
use bray_ir::{MirUnitKey, MirUnitKind};
use bray_runtime_interface::{BinarySymbolName, ExecutableHostContract, ProtectedFrameOperation};
use bray_symbols::{
    AnySymbolId, CallableAbi, ConstantTermData, PackageIdentity, SymbolKey, SymbolKeyData,
};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::binder::CompilationBindingContext;
use super::super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::names::{
    binary_symbol_name, generated_frame_symbol_name, generated_instance_symbol_name,
};
use super::support::{
    codegen_runtime_references, native_boundary_mapping, source_backed_symbol_key, void_signature,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn codegen_symbols(
        &self,
        unit: &CodegenUnit,
        operations: &[CodegenOperationMapping],
        executable_host: Option<&ExecutableHostContract>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut symbols = Vec::new();

        for instance in unit.instances() {
            let (name, linkage, signature) = match instance.mir().kind() {
                MirUnitKind::ExecutableHost(host) => (
                    host.native_entry().clone(),
                    CodegenLinkage::Export,
                    void_signature(CallableAbi::Bray),
                ),
                MirUnitKind::GeneratedLifecycle(reference) => {
                    let name = generated_instance_symbol_name(
                        target,
                        CodegenLinkage::LinkOnce,
                        instance.key(),
                    )?;

                    let signature = self.generated_lifecycle_signature(reference, cancellation)?;

                    (name, CodegenLinkage::LinkOnce, signature)
                }
                MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                    let realization = reachability
                        .instance(instance.key())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let (boundary, linkage) = self.codegen_instance_boundary(
                        instance,
                        roots,
                        platform_overrides,
                        cancellation,
                    )?;

                    let name = match boundary {
                        Some(name) => name,
                        None => self.generated_callable_symbol_name(
                            target,
                            linkage,
                            realization,
                            cancellation,
                        )?,
                    };

                    let signature = self.codegen_instance_signature(realization, cancellation)?;

                    (name, linkage, signature)
                }
            };

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.key().clone()),
                name,
                linkage,
                signature,
            ));
        }

        for instance in unit.external_instances() {
            let realization = reachability
                .instance(instance)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let boundary =
                self.codegen_native_boundary(instance, platform_overrides, cancellation)?;

            let linkage = boundary
                .as_ref()
                .map(|(_, linkage)| *linkage)
                .unwrap_or(CodegenLinkage::Import);

            let name = match boundary {
                Some((name, _)) => name,
                None => {
                    self.generated_callable_symbol_name(target, linkage, realization, cancellation)?
                }
            };

            let signature = self.codegen_instance_signature(realization, cancellation)?;

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.clone()),
                name,
                linkage,
                signature,
            ));
        }

        for reference in codegen_runtime_references(unit, operations, &symbols) {
            let symbol_name = bray_runtime_interface::selected_runtime_role_symbol(
                executable_host,
                reference.role(),
            )
            .ok_or(CodegenPreparationError::MissingRuntimeRole(
                reference.role(),
            ))?;

            let signature = self.codegen_runtime_signature(reference.role())?;

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Runtime(reference),
                symbol_name,
                CodegenLinkage::Import,
                signature,
            ));
        }

        for instance in unit.instances() {
            let Some(frame) = instance.protected_frame_identity() else {
                continue;
            };

            for operation in ProtectedFrameOperation::ALL {
                symbols.push(CodegenSymbolMapping::new(
                    CodegenSymbolKey::ProtectedFrame { frame, operation },
                    generated_frame_symbol_name(target, frame, operation)?,
                    CodegenLinkage::Internal,
                    void_signature(CallableAbi::Bray),
                ));
            }
        }

        for (frame, operation) in operations.iter().flat_map(|mapping| {
            mapping.helpers().iter().filter_map(|helper| {
                let Some(CodegenSymbolKey::ProtectedFrame { frame, operation }) = helper.symbol()
                else {
                    return None;
                };

                Some((*frame, *operation))
            })
        }) {
            let key = CodegenSymbolKey::ProtectedFrame { frame, operation };

            if symbols.iter().any(|symbol| symbol.key() == &key) {
                continue;
            }

            symbols.push(CodegenSymbolMapping::new(
                key,
                generated_frame_symbol_name(target, frame, operation)?,
                CodegenLinkage::Import,
                void_signature(CallableAbi::Bray),
            ));
        }

        Ok(symbols)
    }

    pub(in crate::compilation::product) fn codegen_partition_compatibility(
        &self,
        instance: &CodegenInstance,
        product_package: &PackageIdentity,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<CodegenPartitionCompatibility, CodegenPreparationError> {
        let package =
            self.codegen_instance_package(instance.key(), product_package, cancellation)?;

        let (_, linkage) =
            self.codegen_instance_boundary(instance, roots, &BTreeSet::new(), cancellation)?;

        let visibility = match linkage {
            CodegenLinkage::Private => CodegenDefinitionVisibility::Unit,
            CodegenLinkage::Internal | CodegenLinkage::LinkOnce | CodegenLinkage::Common => {
                CodegenDefinitionVisibility::Product
            }
            CodegenLinkage::External
            | CodegenLinkage::Weak
            | CodegenLinkage::Fallback
            | CodegenLinkage::Import
            | CodegenLinkage::Export => CodegenDefinitionVisibility::Public,
        };

        Ok(CodegenPartitionCompatibility::new(
            package, linkage, visibility,
        ))
    }

    pub(super) fn codegen_instance_boundary(
        &self,
        instance: &CodegenInstance,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        cancellation: &CancellationToken,
    ) -> Result<(Option<BinarySymbolName>, CodegenLinkage), CodegenPreparationError> {
        match instance.mir().kind() {
            MirUnitKind::ExecutableHost(_) => Ok((None, CodegenLinkage::Export)),
            MirUnitKind::GeneratedLifecycle(_) => Ok((None, CodegenLinkage::LinkOnce)),
            MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                let boundary =
                    self.codegen_native_boundary(instance.key(), platform_overrides, cancellation)?;

                if let Some((name, linkage)) = boundary {
                    return Ok((Some(name), linkage));
                }

                let linkage = if roots.contains(instance.key()) {
                    CodegenLinkage::Export
                } else if matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)) {
                    CodegenLinkage::LinkOnce
                } else {
                    CodegenLinkage::Internal
                };

                Ok((None, linkage))
            }
        }
    }

    pub(super) fn codegen_instance_package(
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
            MirUnitKey::GeneratedLifecycle(_) => return Ok(product_package.clone()),
            MirUnitKey::ExternalCallable(definition) => definition.symbol(),
            MirUnitKey::ImportedExecutable(key) => key.owner(),
            MirUnitKey::ExternalRuntimeDefault(symbol) => *symbol,
        };

        let binding_context = self.binding_context(cancellation)?;

        let key = binding_context
            .symbol_key(symbol)
            .map_err(super::super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Compatibility metadata owns package identity beyond the binder-binding_context borrow.
        Ok(key.package_identity().unwrap_or(product_package).clone())
    }

    pub(in crate::compilation::product) fn codegen_native_boundary(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        cancellation: &CancellationToken,
    ) -> Result<Option<(BinarySymbolName, CodegenLinkage)>, CodegenPreparationError> {
        if matches!(
            instance.template(),
            MirUnitKey::GeneratedLifecycle(_)
                | MirUnitKey::ExecutableHost(_)
                | MirUnitKey::ExternalRuntimeDefault(_)
        ) {
            return Ok(None);
        }

        if match instance.template() {
            MirUnitKey::Bound(key) => {
                key.declared_owner().kind() == bray_symbols::SymbolKind::Static
            }
            MirUnitKey::ImportedExecutable(key) => {
                matches!(key.owner(), AnySymbolId::Static(_))
            }
            _ => false,
        } {
            return Ok(None);
        }

        if self
            .codegen_runtime_default_provider(instance, cancellation)?
            .is_some()
        {
            return Ok(None);
        }

        let definition = self.codegen_callable_definition(instance)?;

        let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol()
        else {
            return Ok(None);
        };

        let contract = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        if let Some(contract) = contract.value() {
            let name = contract
                .symbol()
                .identity()
                .name()
                .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            return native_boundary_mapping(
                name,
                contract.direction(),
                contract.symbol().binding(),
            )
            .map(Some);
        }

        let platform_service = match instance.template() {
            MirUnitKey::Bound(_) => {
                crate::compilation::foreign::platform::platform_service_role(self, function)?
            }
            MirUnitKey::ImportedExecutable(key) => key.platform_service(),
            MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::ExecutableHost(_)
            | MirUnitKey::ExternalCallable(_)
            | MirUnitKey::ExternalRuntimeDefault(_) => None,
        };

        if let Some(role) = platform_service {
            let name = BinarySymbolName::try_new(
                bray_runtime_interface::native_platform_service_role_symbol(role),
            )
            .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            let linkage = if platform_overrides.contains(&role) {
                CodegenLinkage::Import
            } else {
                CodegenLinkage::Fallback
            };

            return Ok(Some((name, linkage)));
        }

        let boundary =
            self.imported_native_boundary_with_cancellation(function.into(), cancellation)?;

        boundary
            .map(|boundary| {
                let name = boundary
                    .symbol()
                    .identity()
                    .name()
                    .ok_or(CodegenPreparationError::InvalidSymbolName)?;

                native_boundary_mapping(name, boundary.direction(), boundary.symbol().binding())
            })
            .transpose()
    }

    pub(super) fn generated_callable_symbol_name(
        &self,
        target: &CodegenTarget,
        linkage: CodegenLinkage,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<BinarySymbolName, CodegenPreparationError> {
        if let Some(provider) =
            self.codegen_runtime_default_provider(realization.key(), cancellation)?
        {
            let binding_context = self.binding_context(cancellation)?;
            let provider = self.portable_codegen_symbol_key(&binding_context, provider)?;
            let mut hasher = StableDigestHasher::new();

            hasher.write(b"bray.codegen-runtime-default-symbol");
            provider.hash(&mut hasher);

            realization.key().specialization().hash(&mut hasher);
            realization.key().witnesses().hash(&mut hasher);
            realization.key().target().hash(&mut hasher);

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

        realization.key().specialization().hash(&mut hasher);
        realization.key().witnesses().hash(&mut hasher);
        realization.key().target().hash(&mut hasher);

        binary_symbol_name(target, linkage, "instance", hasher.finalize())
    }

    pub(super) fn codegen_runtime_default_provider(
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
                    .ok_or_else(|| FactQueryError::InfrastructureFailure.into())
            }
            MirUnitKey::ImportedExecutable(provider) => {
                let binding_context = self.binding_context(cancellation)?;
                let provider = provider.owner();

                Ok(binding_context
                    .runtime_default_subject(provider)
                    .map(|subject| subject.map(|_| provider))
                    .map_err(super::super::super::binder::binding_query_error)?)
            }
            MirUnitKey::ExternalRuntimeDefault(provider) => Ok(Some(*provider)),
            MirUnitKey::Bound(_)
            | MirUnitKey::ExecutableHost(_)
            | MirUnitKey::GeneratedLifecycle(_)
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
            .map_err(super::super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        match definition.data() {
            SymbolKeyData::SourceDeclaration { .. } => Ok(SymbolKey::external(
                super::super::super::export::external_symbol_key(
                    binding_context.symbols(),
                    self.package_identity(),
                    symbol,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)?,
            )),
            SymbolKeyData::Synthesized(synthesized)
                if source_backed_symbol_key(synthesized.subject()) =>
            {
                Ok(SymbolKey::external(
                    super::super::super::export::external_symbol_key(
                        binding_context.symbols(),
                        self.package_identity(),
                        symbol,
                    )
                    .map_err(|_| FactQueryError::InfrastructureFailure)?,
                ))
            }
            // Symbol keys are Arc-backed and the generated mapping owns its identity input.
            _ => Ok(definition.clone()),
        }
    }

    pub(super) fn codegen_callables(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCallableMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for (owner, demand) in demanded_callable_instances(unit) {
            let owner_realization = reachability
                .instance(&owner)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mapping = match self.concrete_codegen_callee(
                owner_realization,
                &demand,
                target,
                cancellation,
            )? {
                ConcreteCodegenCallee::Instance(instance) => CodegenCallableMapping::new(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    instance.key().clone(),
                ),
                ConcreteCodegenCallee::Intrinsic(intrinsic) => CodegenCallableMapping::intrinsic(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    intrinsic,
                ),
            };

            mappings.push(mapping);
        }

        Ok(mappings)
    }

    pub(super) fn codegen_constants(
        &self,
        unit: &CodegenUnit,
        additional: impl IntoIterator<Item = bray_symbols::ConstantValueId>,
    ) -> Result<Vec<CodegenConstantMapping>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let demands = demanded_constants(unit);
        let mut pending = Vec::new();
        let mut mapped = BTreeMap::new();

        for value in demands.values() {
            match demands.types().get(value) {
                Some(types) if !types.is_empty() => {
                    pending.extend(types.iter().map(|ty| (*value, Some(*ty))));
                }
                Some(_) | None => pending.push((*value, None)),
            }
        }

        pending.extend(additional.into_iter().map(|value| (value, None)));

        while let Some((value, representation)) = pending.pop() {
            let data = values
                .constant_value_data(value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let representation = representation.unwrap_or_else(|| data.ty());
            let key = (value, representation);

            if mapped.contains_key(&key) {
                continue;
            }

            pending.extend(child_constants(data.kind()).map(|child| (child, None)));

            // Code generation mappings outlive this shared semantic-store read and therefore
            // take independent ownership of the immutable payload at this boundary.
            mapped.insert(
                key,
                CodegenConstantMapping::with_representation(
                    value,
                    data.as_ref().clone(),
                    representation,
                ),
            );
        }

        Ok(mapped.into_values().collect())
    }

    pub(super) fn codegen_constant_terms(
        &self,
        unit: &CodegenUnit,
        reachability: &ConcreteCodegenReachability,
    ) -> Result<
        (
            Vec<CodegenConstantTermMapping>,
            Vec<CodegenTerminatorMapping>,
        ),
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let mut terms = Vec::new();
        let mut resolved = BTreeMap::new();

        for instance in unit.instances() {
            let realization = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for template_term in demanded_constant_terms(instance.mir()) {
                let term = self
                    .substitute_codegen_constant_term(template_term, realization.substitution())?;

                let data = values
                    .constant_term_data(term)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let ConstantTermData::Value(value) = data.as_ref() else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
                };

                resolved.insert((instance.key().clone(), template_term), *value);

                terms.push(CodegenConstantTermMapping::new(
                    instance.key().clone(),
                    template_term,
                    *value,
                ));
            }
        }

        let mut terminators = Vec::new();

        for instance in unit.instances() {
            for (block, data) in instance.mir().blocks_with_ids() {
                if let Some(value) = data.terminator().kind().pattern_literal_value() {
                    terminators.push(CodegenTerminatorMapping::new(
                        instance.key().clone(),
                        block,
                        [value],
                    ));

                    continue;
                }

                let Some(term) = data.terminator().kind().pattern_constant_term() else {
                    continue;
                };

                let Some(value) = resolved.get(&(instance.key().clone(), term)) else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
                };

                terminators.push(CodegenTerminatorMapping::new(
                    instance.key().clone(),
                    block,
                    [*value],
                ));
            }
        }

        Ok((terms, terminators))
    }
}
