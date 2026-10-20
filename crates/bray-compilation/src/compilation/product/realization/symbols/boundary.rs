use crate::compilation::product::realization::names::native_entry_name;
use crate::compilation::product::realization::support::native_boundary_mapping;
use crate::compilation::product::specialization::{
    ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;
use bray_binder::SymbolQueryProvider;
use bray_codegen::{
    CodegenDefinitionVisibility, CodegenInstance, CodegenLinkage, CodegenNativeEntryMapping,
    CodegenPartitionCompatibility, CodegenTarget,
};
use bray_ir::{MirStorageKind, MirUnitKey, MirUnitKind};
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{AnySymbolId, CallableAbi, ProductIdentity};
use std::collections::BTreeSet;

const CALLBACK_BODY_LINKAGE: CodegenLinkage = CodegenLinkage::LinkOnce;

fn default_instance_linkage(
    instance: &CodegenInstance,
    roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
) -> CodegenLinkage {
    // Package-portable bodies may be emitted by their producer and consumers.
    match instance.key().template() {
        MirUnitKey::ImportedExecutable(_) | MirUnitKey::GeneratedLifecycle(_) => {
            CodegenLinkage::LinkOnce
        }
        MirUnitKey::Bound(unit)
            if unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault =>
        {
            CodegenLinkage::LinkOnce
        }
        _ if roots.contains(instance.key()) => CodegenLinkage::Export,
        _ => CodegenLinkage::Internal,
    }
}

pub(in crate::compilation::product) enum NativeBoundaryMapping {
    Direct {
        name: BinarySymbolName,
        linkage: CodegenLinkage,
    },
    Callback {
        name: Option<BinarySymbolName>,
        linkage: CodegenLinkage,
        kind: bray_runtime_interface::NativeEntryKind,
    },
}

impl NativeBoundaryMapping {
    const fn definition_linkage(&self) -> CodegenLinkage {
        match self {
            Self::Direct { linkage, .. } => *linkage,
            Self::Callback { .. } => CALLBACK_BODY_LINKAGE,
        }
    }
}

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "symbol boundaries require the existing product, target, and reachability contracts"
    )]
    pub(in crate::compilation::product) fn codegen_emitted_symbol_boundary(
        &self,
        product: &ProductIdentity,
        instance: &CodegenInstance,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BinarySymbolName,
            CodegenLinkage,
            Option<CodegenNativeEntryMapping>,
        ),
        CodegenPreparationError,
    > {
        match instance.mir().kind() {
            MirUnitKind::ExecutableHost(host) => {
                Ok((host.native_entry().clone(), CodegenLinkage::Export, None))
            }
            MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                let realization = reachability
                    .instance(instance.key())
                    .expect("retained codegen instance must have a concrete realization");

                let boundary =
                    self.codegen_native_boundary(realization, platform_overrides, cancellation)?;

                self.codegen_callable_symbol_boundary(
                    product,
                    target,
                    realization,
                    boundary,
                    default_instance_linkage(instance, roots),
                    cancellation,
                )
            }
        }
    }

    pub(in crate::compilation::product) fn codegen_callable_symbol_boundary(
        &self,
        product: &bray_symbols::ProductIdentity,
        target: &CodegenTarget,
        realization: &ConcreteCodegenInstance,
        boundary: Option<NativeBoundaryMapping>,
        default_linkage: CodegenLinkage,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BinarySymbolName,
            CodegenLinkage,
            Option<CodegenNativeEntryMapping>,
        ),
        CodegenPreparationError,
    > {
        match boundary {
            Some(NativeBoundaryMapping::Callback {
                name,
                linkage,
                kind,
            }) => {
                let body_name = self.generated_callable_symbol_name(
                    product,
                    target,
                    CALLBACK_BODY_LINKAGE,
                    realization,
                    cancellation,
                )?;

                let entry_name = name.unwrap_or_else(|| native_entry_name(&body_name));

                Ok((
                    body_name,
                    CALLBACK_BODY_LINKAGE,
                    Some(CodegenNativeEntryMapping::new(entry_name, linkage).with_kind(kind)),
                ))
            }
            Some(NativeBoundaryMapping::Direct { name, linkage }) => Ok((name, linkage, None)),
            None => self
                .generated_callable_symbol_name(
                    product,
                    target,
                    default_linkage,
                    realization,
                    cancellation,
                )
                .map(|name| (name, default_linkage, None)),
        }
    }

    pub(in crate::compilation::product) fn codegen_partition_compatibility(
        &self,
        instance: &CodegenInstance,
        realization: &ConcreteCodegenInstance,
        product: &ProductIdentity,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<CodegenPartitionCompatibility, CodegenPreparationError> {
        let package =
            self.codegen_instance_package(instance.key(), product.package(), cancellation)?;

        let linkage = match instance.mir().kind() {
            MirUnitKind::ExecutableHost(_) => CodegenLinkage::Export,
            MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => self
                .codegen_native_boundary(realization, &BTreeSet::new(), cancellation)?
                .map_or_else(
                    || default_instance_linkage(instance, roots),
                    |boundary| boundary.definition_linkage(),
                ),
        };

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

        // Imported anchors carry their producer namespace in MIR, but static
        // mappings can still attach a consumer-local finalizer source.
        let source_namespace = match instance.key().template() {
            MirUnitKey::ImportedExecutable(_)
                if !instance.mir().storages().iter().any(|storage| {
                    matches!(
                        storage.kind(),
                        MirStorageKind::Static(_) | MirStorageKind::NativeStatic(_)
                    )
                }) =>
            {
                [0; 32]
            }
            _ => product.source_namespace(),
        };

        let (independent, storage_dependencies) =
            self.codegen_native_publication_metadata(instance, realization, cancellation)?;

        let mut compatibility =
            CodegenPartitionCompatibility::new(package, source_namespace, linkage, visibility)
                .with_native_storage_dependencies_identity(storage_dependencies);

        if independent {
            compatibility = compatibility.with_native_selection_boundary();
        }

        Ok(compatibility)
    }

    pub(in crate::compilation::product) fn codegen_selected_native_boundary(
        &self,
        selected: &crate::compilation::product::codegen::reuse::SelectedNativeUnit,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BinarySymbolName,
            CodegenLinkage,
            Option<CodegenNativeEntryMapping>,
        ),
        CodegenPreparationError,
    > {
        let signature = self.codegen_instance_signature(realization, cancellation)?;

        let entry = (signature.abi() != CallableAbi::Bray && signature.has_panic_report_context())
            .then(|| {
                CodegenNativeEntryMapping::new(
                    native_entry_name(&selected.symbol),
                    CodegenLinkage::Import,
                )
            });

        // Explicit native exports are selected through their published boundary instead.
        Ok((selected.symbol.clone(), CodegenLinkage::Import, entry))
    }

    pub(in crate::compilation::product) fn codegen_native_boundary(
        &self,
        realization: &ConcreteCodegenInstance,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        cancellation: &CancellationToken,
    ) -> Result<Option<NativeBoundaryMapping>, CodegenPreparationError> {
        if let Some(ty) = realization.anonymous_callable_type() {
            let data = self.semantic_value_store()?.type_data(ty);

            let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                panic!("anonymous callable realization must retain its callable type");
            };

            return Ok((callable.abi() != CallableAbi::Bray).then_some(
                NativeBoundaryMapping::Callback {
                    name: None,
                    linkage: CodegenLinkage::LinkOnce,
                    kind: bray_runtime_interface::NativeEntryKind::ForeignCallback,
                },
            ));
        }

        let instance = realization.key();

        if matches!(
            instance.template(),
            MirUnitKey::GeneratedLifecycle(_)
                | MirUnitKey::ExecutableHost(_)
                | MirUnitKey::Predicate(_)
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

        if let MirUnitKey::Bound(_) = instance.template()
            && let Some(role) = crate::compilation::foreign::runtime::runtime_role(self, function)?
        {
            let symbol = role
                .native_symbol()
                .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            let name = BinarySymbolName::try_new(symbol)
                .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            if let Some(kind) = bray_runtime_interface::NativeEntryKind::for_runtime_role(role) {
                return Ok(Some(NativeBoundaryMapping::Callback {
                    name: Some(name),
                    linkage: CodegenLinkage::Export,
                    kind,
                }));
            }

            return Ok(Some(NativeBoundaryMapping::Direct {
                name,
                linkage: CodegenLinkage::Export,
            }));
        }

        let platform_service = match instance.template() {
            MirUnitKey::Bound(_) => {
                crate::compilation::foreign::platform::platform_service_role(self, function)?
            }
            MirUnitKey::ImportedExecutable(key) => key.platform_service(),
            MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::ExecutableHost(_)
            | MirUnitKey::ExternalCallable(_)
            | MirUnitKey::CompilerProvidedCallable(_)
            | MirUnitKey::Predicate(_)
            | MirUnitKey::ExternalRuntimeDefault(_) => None,
        };

        if let Some(role) = platform_service {
            let name = BinarySymbolName::try_new(role.native_symbol())
                .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            let linkage = if platform_overrides.contains(&role) {
                CodegenLinkage::Import
            } else {
                CodegenLinkage::Fallback
            };

            return Ok(Some(NativeBoundaryMapping::Direct { name, linkage }));
        }

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

        let boundary =
            self.imported_native_boundary_with_cancellation(function.into(), cancellation)?;

        if let Some(boundary) = boundary {
            let name = boundary
                .symbol()
                .identity()
                .name()
                .ok_or(CodegenPreparationError::InvalidSymbolName)?;

            return native_boundary_mapping(
                name,
                boundary.direction(),
                boundary.symbol().binding(),
            )
            .map(Some);
        }

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableSignatureQuery,
            >::new(function.into()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let values = self.semantic_value_store()?;

        let abi = match signature.value().callable_type() {
            bray_symbols::TypeExpressionTemplate::Callable(callable) => callable.abi(),
            bray_symbols::TypeExpressionTemplate::Resolved(ty) => {
                let data = values.type_data(*ty);

                let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                    panic!("callable signature must retain a callable type");
                };

                callable.abi()
            }
            _ => panic!("callable signature must retain a callable type"),
        };

        Ok(
            (signature.value().invocation() == bray_symbols::CallableInvocation::Bray
                && abi != CallableAbi::Bray)
                .then_some(NativeBoundaryMapping::Callback {
                    name: None,
                    linkage: CodegenLinkage::LinkOnce,
                    kind: bray_runtime_interface::NativeEntryKind::ForeignCallback,
                }),
        )
    }

    pub(in crate::compilation::product::realization) fn codegen_runtime_source_role(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
    ) -> Result<Option<bray_runtime_interface::RuntimeAbiRole>, CodegenPreparationError> {
        let MirUnitKey::Bound(unit) = instance.template() else {
            return Ok(None);
        };

        if self.runtime_roles().is_empty()
            || unit.kind() != bray_bound_tree::BoundUnitKind::CallableBody
        {
            return Ok(None);
        }

        let definition = self.codegen_callable_definition(instance)?;

        let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol()
        else {
            return Ok(None);
        };

        crate::compilation::foreign::runtime::runtime_role(self, function).map_err(Into::into)
    }
}
