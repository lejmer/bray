use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{
    CodegenCallSite, CodegenHelperMapping, CodegenInstance, CodegenOperationMapping,
    CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget, CodegenTypeMapping, CodegenUnit,
    demanded_callable_instance_for_call,
};
use bray_ir::{
    MirAsyncOperation, MirCallTarget, MirFrameInitializer, MirHelperReference, MirOperationId,
    MirOperationKind, MirUnit, MirUnitId, MirUnitKey,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::support::{
    dependency_symbol, direct_helper_symbol, helper_runtime_symbol, operation_result_type,
};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn codegen_operations(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenOperationMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for instance in unit.instances() {
            let realization = reachability.instance(instance.key()).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::ConcreteInstance,
                )
            })?;

            for (operation, data) in instance.mir().operations_with_ids() {
                let references = data.kind().helper_references();

                let outgoing = self.operation_outgoing_capacity(
                    realization,
                    data.kind(),
                    target,
                    cancellation,
                )?;

                if references.is_empty() && outgoing.is_none() {
                    continue;
                }

                let helpers = references
                    .into_iter()
                    .map(|reference| {
                        self.codegen_helper(
                            instance,
                            operation,
                            data.kind(),
                            operation_result_type(instance.mir(), data),
                            realization,
                            reference,
                            target,
                            cancellation,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                mappings.push(
                    CodegenOperationMapping::new(instance.key().clone(), operation, helpers)
                        .with_outgoing_capacity(outgoing),
                );
            }
        }

        Ok(mappings)
    }

    pub(super) fn classify_codegen_symbols(
        &self,
        symbols: Vec<CodegenSymbolMapping>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut classified = Vec::with_capacity(symbols.len());
        let mut pending = BTreeSet::new();

        for symbol in symbols {
            let (key, name, linkage, signature, native_entry) = symbol.into_parts();

            let signature = self.classify_codegen_signature(
                signature,
                target,
                cancellation,
                mappings,
                &mut pending,
            )?;

            let mut symbol = CodegenSymbolMapping::new(key, name, linkage, signature);

            if let Some(native_entry) = native_entry {
                symbol = symbol.with_native_entry(native_entry);
            }

            classified.push(symbol);
        }

        Ok(classified)
    }

    pub(super) fn codegen_helper(
        &self,
        owner: &CodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        operation_result_type: Option<TypeId>,
        owner_realization: &ConcreteCodegenInstance,
        reference: MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenHelperMapping, CodegenPreparationError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner_realization, &reference, cancellation)?;

        if concrete_reference.lifecycle_type().is_some()
            && self.codegen_lifecycle_is_trivial(&concrete_reference, cancellation)?
        {
            return Ok(CodegenHelperMapping::lowered(reference));
        }

        if let Some(symbol) = direct_helper_symbol(owner, &reference) {
            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        if let Some(dependency) = self.concrete_codegen_helper_dependency(
            owner_realization,
            operation_id,
            operation,
            operation_result_type,
            &reference,
            target,
            cancellation,
        )? {
            return dependency_symbol(owner, dependency.key(), &reference)
                .map(|symbol| CodegenHelperMapping::new(reference, symbol));
        }

        if matches!(reference, MirHelperReference::CreateFrame(_)) {
            let symbol = self.frame_creation_symbol(
                owner,
                owner_realization,
                operation_id,
                operation,
                &reference,
                target,
                cancellation,
            )?;

            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        Err(CodegenPreparationError::MissingHelperInstance(reference))
    }

    pub(in crate::compilation::product) fn concrete_codegen_helper_dependency(
        &self,
        owner: &ConcreteCodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        operation_result_type: Option<TypeId>,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCodegenInstance>, CodegenPreparationError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner, reference, cancellation)?;

        let dependency = match &concrete_reference {
            MirHelperReference::AnonymousCallable(unit) => {
                let callable_type = operation_result_type.ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::Operation {
                            instance: owner.key().clone(),
                            operation: operation_id,
                        },
                        ProductDataKind::OperationResultType,
                    )
                })?;

                self.concrete_codegen_anonymous_callable(owner, unit, callable_type)?
            }
            MirHelperReference::DeclaredCallable(callable) => self.concrete_codegen_callable_data(
                owner,
                &callable.instance(),
                target,
                cancellation,
            )?,
            MirHelperReference::DefaultValue(provider) => self.concrete_codegen_default_value(
                owner,
                operation_id,
                operation,
                *provider,
                target,
                cancellation,
            )?,
            MirHelperReference::TypeForm(callable) | MirHelperReference::Conversion(callable) => {
                self.concrete_codegen_callable_data(owner, callable, target, cancellation)?
            }
            MirHelperReference::StandardLibrary(helper) => {
                self.concrete_standard_library_helper(*helper, target, cancellation)?
            }
            MirHelperReference::Finalize(_)
            | MirHelperReference::StaticFinalize(_)
            | MirHelperReference::Destroy(_)
            | MirHelperReference::Cleanup { .. } => {
                if self.codegen_lifecycle_is_trivial(&concrete_reference, cancellation)? {
                    return Ok(None);
                }

                self.concrete_codegen_lifecycle(concrete_reference, target)?
            }
            MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => return Ok(None),
        };

        Ok(Some(dependency))
    }

    pub(super) fn concrete_codegen_helper_reference(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<MirHelperReference, CodegenPreparationError> {
        let substitution = owner.substitution();

        Ok(match reference {
            MirHelperReference::Finalize(ty) => MirHelperReference::Finalize(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::StaticFinalize(ty) => MirHelperReference::StaticFinalize(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::Destroy(ty) => MirHelperReference::Destroy(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::Cleanup { phase, ty } => MirHelperReference::Cleanup {
                phase: *phase,
                ty: self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            },
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::DefaultValue(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::StandardLibrary(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => reference.clone(),
        })
    }

    pub(super) fn frame_creation_symbol(
        &self,
        owner: &CodegenInstance,
        owner_realization: &ConcreteCodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenSymbolKey, CodegenPreparationError> {
        let MirOperationKind::Async(MirAsyncOperation::CreateFrame { initializer, .. }) = operation
        else {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        };

        match initializer {
            MirFrameInitializer::Callable(call) => match call.target() {
                MirCallTarget::Direct(_) => {
                    let demand = demanded_callable_instance_for_call(
                        CodegenCallSite::Operation(operation_id),
                        call,
                    )
                    .ok_or_else(|| {
                        CodegenPreparationError::MissingHelperInstance(reference.clone())
                    })?;

                    let ConcreteCodegenCallee::Instance(dependency) = self
                        .concrete_codegen_callee(
                            owner_realization,
                            &demand,
                            target,
                            cancellation,
                        )?
                    else {
                        return Err(CodegenPreparationError::MissingHelperInstance(
                            reference.clone(),
                        ));
                    };

                    dependency_symbol(owner, dependency.key(), reference)
                }
                MirCallTarget::Indirect { .. } => {
                    Ok(helper_runtime_symbol(owner, RuntimeAbiRole::FrameCreation))
                }
                MirCallTarget::Runtime(_) | MirCallTarget::DefaultValue { .. } => Err(
                    CodegenPreparationError::MissingHelperInstance(reference.clone()),
                ),
            },
            MirFrameInitializer::TaskObservation { .. } => Ok(helper_runtime_symbol(
                owner,
                RuntimeAbiRole::TaskObservationCreation,
            )),
        }
    }

    pub(in crate::compilation) fn codegen_generated_lifecycle_mir(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        reference: &MirHelperReference,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let MirUnitKey::GeneratedLifecycle(key) = instance.template() else {
            return Err(ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.clone()),
                ProductDataKind::GeneratedLifecycleInstance,
            )
            .into());
        };

        let actual_role = bray_ir::MirGeneratedLifecycleRole::from_reference(reference);

        if Some(key.role()) != actual_role {
            return Err(ProductQueryFailure::LifecycleRoleMismatch {
                instance: instance.clone(),
                expected: Some(key.role()),
                actual: actual_role,
            }
            .into());
        }

        let context =
            super::synthetic::CompilationSyntheticLoweringContext::new(self, cancellation)?;

        bray_lowering::lower_lifecycle(
            &context,
            instance.template().clone(),
            reference,
            unit,
            instance.target(),
        )
    }
}
