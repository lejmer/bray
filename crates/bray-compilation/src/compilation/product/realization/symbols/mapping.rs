use crate::compilation::product::realization::names::generated_frame_symbol_name;
use crate::compilation::product::realization::support::void_signature;
use crate::compilation::product::specialization::ConcreteCodegenReachability;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;
use bray_codegen::{
    CodegenLinkage, CodegenOperationMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenUnit, mapped_runtime_references,
};
use bray_ir::MirUnitKind;
use bray_runtime_interface::{ExecutableHostContract, ProtectedFrameOperation};
use bray_symbols::CallableAbi;
use std::collections::BTreeSet;

fn assert_symbol_contract(target: &CodegenTarget, symbol: &CodegenSymbolMapping) {
    assert!(
        target.symbols().supports(symbol.linkage()),
        "codegen symbol {:?} uses unsupported {:?} linkage for target {:?}",
        symbol.key(),
        symbol.linkage(),
        target.identity()
    );

    let Some(entry) = symbol.native_entry() else {
        return;
    };

    assert!(
        target.symbols().supports(entry.linkage()),
        "native entry {:?} for codegen symbol {:?} uses unsupported {:?} linkage for target {:?}",
        entry.name(),
        symbol.key(),
        entry.linkage(),
        target.identity()
    );

    assert!(
        matches!(symbol.key(), CodegenSymbolKey::Instance(_))
            && symbol.name() != entry.name()
            && ((symbol.linkage() == CodegenLinkage::Import)
                == (entry.linkage() == CodegenLinkage::Import))
            && matches!(
                entry.linkage(),
                CodegenLinkage::Export
                    | CodegenLinkage::Weak
                    | CodegenLinkage::LinkOnce
                    | CodegenLinkage::Import
            )
            && symbol.signature().abi() != CallableAbi::Bray,
        "native entry {:?} with {:?} linkage contradicts codegen symbol {:?} with name {:?}, {:?} linkage, and {:?} ABI",
        entry.name(),
        entry.linkage(),
        symbol.key(),
        symbol.name(),
        symbol.linkage(),
        symbol.signature().abi()
    );
}

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "symbol mapping combines checked operations, static storage, host, platform overrides, roots, and reachability"
    )]
    pub(in crate::compilation::product::realization) fn codegen_symbols(
        &self,
        product: &bray_symbols::ProductIdentity,
        unit: &CodegenUnit,
        operations: &[CodegenOperationMapping],
        static_storages: &[bray_codegen::CodegenStaticStorageMapping],
        callables: &[bray_codegen::CodegenCallableMapping],
        executable_host: Option<&ExecutableHostContract>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut symbols = Vec::new();

        for instance in unit.instances() {
            let (name, linkage, native_entry) = self.codegen_emitted_symbol_boundary(
                product,
                instance,
                platform_overrides,
                target,
                roots,
                reachability,
                cancellation,
            )?;

            let signature = match instance.mir().kind() {
                MirUnitKind::ExecutableHost(_) => void_signature(CallableAbi::Bray),
                MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                    let realization = reachability
                        .instance(instance.key())
                        .expect("retained codegen instance must have a concrete realization");

                    let mut signature =
                        self.codegen_instance_signature(realization, cancellation)?;

                    if let Some(role) = self.codegen_runtime_source_role(instance.key())? {
                        signature = crate::compilation::product::realization::runtime_source::checked_runtime_source_signature(
                            self,
                            role,
                            instance.key(),
                            signature,
                            cancellation,
                        )?;
                    }

                    signature
                }
            };

            let mut symbol = CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.key().clone()),
                name,
                linkage,
                signature,
            );

            if let Some(native_entry) = native_entry {
                symbol = symbol.with_native_entry(native_entry);
            }

            assert_symbol_contract(target, &symbol);
            symbols.push(symbol);
        }

        for instance in unit.external_instances() {
            let realization = reachability.instance(instance).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.clone()),
                    ProductDataKind::ConcreteInstance,
                )
            })?;

            let (name, linkage, native_entry) = match reachability.selected_native(instance) {
                // The mapping owns the published symbol after reachability is released.
                Some(selected) => {
                    self.codegen_selected_native_boundary(selected, realization, cancellation)?
                }
                None => {
                    let boundary = self.codegen_native_boundary(
                        realization,
                        platform_overrides,
                        cancellation,
                    )?;

                    self.codegen_callable_symbol_boundary(
                        product,
                        target,
                        realization,
                        boundary,
                        CodegenLinkage::Import,
                        cancellation,
                    )?
                }
            };

            let signature = self.codegen_instance_signature(realization, cancellation)?;

            let mut symbol = CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.clone()),
                name,
                linkage,
                signature,
            );

            if let Some(native_entry) = native_entry {
                symbol = symbol.with_native_entry(native_entry);
            }

            assert_symbol_contract(target, &symbol);
            symbols.push(symbol);
        }

        for reference in mapped_runtime_references(unit, &symbols, callables) {
            let symbol_name = bray_runtime_interface::selected_runtime_role_symbol(
                executable_host,
                reference.role(),
            )
            .unwrap_or_else(|| {
                panic!(
                    "checked native MissingRuntimeRole contract failed: {:?}",
                    reference.role(),
                )
            });

            let signature = self.codegen_runtime_signature(reference.role())?;

            let symbol = CodegenSymbolMapping::new(
                CodegenSymbolKey::Runtime(reference),
                symbol_name,
                CodegenLinkage::Import,
                signature,
            );

            assert_symbol_contract(target, &symbol);
            symbols.push(symbol);
        }

        for instance in unit.instances() {
            let Some(frame) = instance.protected_frame_identity() else {
                continue;
            };

            for operation in ProtectedFrameOperation::ALL {
                let symbol = CodegenSymbolMapping::new(
                    CodegenSymbolKey::ProtectedFrame { frame, operation },
                    generated_frame_symbol_name(target, frame, operation)?,
                    CodegenLinkage::Internal,
                    void_signature(CallableAbi::Bray),
                );

                assert_symbol_contract(target, &symbol);
                symbols.push(symbol);
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

            let symbol = CodegenSymbolMapping::new(
                key,
                generated_frame_symbol_name(target, frame, operation)?,
                CodegenLinkage::Import,
                void_signature(CallableAbi::Bray),
            );

            assert_symbol_contract(target, &symbol);
            symbols.push(symbol);
        }

        append_frame_data_symbols(unit, operations, static_storages, target, &mut symbols)?;

        Ok(symbols)
    }
}

fn append_frame_data_symbols(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
    static_storages: &[bray_codegen::CodegenStaticStorageMapping],
    target: &CodegenTarget,
    symbols: &mut Vec<CodegenSymbolMapping>,
) -> Result<(), CodegenPreparationError> {
    let mut storage = unit
        .instances()
        .iter()
        .filter(|instance| instance.protected_frame_identity().is_some())
        .map(|instance| instance.key().clone())
        .collect::<BTreeSet<_>>();

    storage.extend(
        operations
            .iter()
            .filter_map(CodegenOperationMapping::outgoing_requirements)
            .flatten()
            .chain(
                static_storages
                    .iter()
                    .flat_map(bray_codegen::CodegenStaticStorageMapping::outgoing_requirements),
            )
            .filter(|requirement| {
                requirement.execution() == bray_symbols::CallableExecution::Asynchronous
            })
            .map(|requirement| requirement.callable().clone()),
    );

    for operation in unit
        .instances()
        .iter()
        .flat_map(|instance| instance.mir().operations())
    {
        if let bray_ir::MirOperationKind::Host(bray_ir::MirHostOperation::ExecuteRoot {
            root,
            execution: bray_runtime_interface::RootExecution::Asynchronous { .. },
            ..
        }) = operation.kind()
        {
            let instance = unit.executable_root_instance(root);

            storage.insert(instance.clone());
        }
    }

    storage.extend(
        operations
            .iter()
            .flat_map(|mapping| mapping.helpers())
            .filter_map(|helper| match helper.symbol() {
                Some(CodegenSymbolKey::FrameFactory(instance)) => Some(instance.clone()),
                _ => None,
            }),
    );

    for instance in storage {
        let linkage = if unit
            .instances()
            .iter()
            .any(|member| member.key() == &instance)
        {
            CodegenLinkage::LinkOnce
        } else {
            CodegenLinkage::Import
        };

        // Native imports keep the producer's constructor name. Its companion data
        // must use that same identity rather than the consumer's imported MIR key.
        let constructor = symbols.iter().find(|mapping| {
            matches!(mapping.key(), CodegenSymbolKey::Instance(key) if key == &instance)
        }).unwrap_or_else(|| panic!("frame companion data for {:?} requires its mapped constructor in {:?}",
            instance.template(), unit.instances().iter().map(|member| member.key().template()).collect::<Vec<_>>()));

        let backing = crate::compilation::product::realization::names::generated_symbol_name(
            target,
            linkage,
            "cleanup_storage",
            constructor.name(),
        )?;

        let factory = crate::compilation::product::realization::names::generated_symbol_name(
            target,
            linkage,
            "frame_factory",
            constructor.name(),
        )?;

        symbols.push(CodegenSymbolMapping::new(
            CodegenSymbolKey::CleanupStorage(instance.clone()),
            backing,
            linkage,
            void_signature(CallableAbi::C),
        ));

        symbols.push(CodegenSymbolMapping::new(
            CodegenSymbolKey::FrameFactory(instance),
            factory,
            linkage,
            void_signature(CallableAbi::C),
        ));
    }

    Ok(())
}
