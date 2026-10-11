use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
use bray_codegen::CodegenTarget;
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirCallTarget, MirCleanupPhase, MirHelperReference, MirOperationKind};
use bray_lowering::SyntheticLoweringContext;
use bray_symbols::{CallableExecution, CallableInstanceData, TypeId};

use super::super::specialization::ConcreteCodegenInstance;
use super::synthetic::CompilationSyntheticLoweringContext;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn operation_outgoing_capacity(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<u32>, CodegenPreparationError> {
        match operation {
            MirOperationKind::AdmitOutgoing { ty, .. }
            | MirOperationKind::DischargeOutgoing { ty, .. } => {
                let ty = self.concrete_codegen_type(
                    *ty,
                    owner.substitution(),
                    Some(owner),
                    cancellation,
                )?;

                self.owner_outgoing_capacity(ty, cancellation).map(Some)
            }
            MirOperationKind::Call(call) if call.is_cleanup() => {
                let MirCallTarget::Direct(reference) = call.target() else {
                    return Err(ProductQueryFailure::InvalidCleanupCallTarget {
                        context: ProductQueryContext::Instance(owner.key().clone()),
                        // Retain the rejected target only while constructing its diagnostic.
                        target: call.target().clone(),
                    }
                    .into());
                };

                let callee = self.concrete_codegen_callable_data(
                    owner,
                    &reference.instance(),
                    target,
                    cancellation,
                )?;

                let callable = callee.callable_instance().ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::Instance(callee.key().clone()),
                        ProductDataKind::ConcreteInstance,
                    )
                })?;

                let result = self.concrete_codegen_type(
                    call.result().ty(),
                    owner.substitution(),
                    Some(owner),
                    cancellation,
                )?;

                self.call_outgoing_capacity(
                    callable,
                    CallableExecution::Synchronous,
                    result,
                    cancellation,
                )
                .map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn owner_outgoing_capacity(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<u32, CodegenPreparationError> {
        let context = CompilationSyntheticLoweringContext::new(self, cancellation)?;
        let mut capacity = 0;

        for (phase, helper) in [
            (LifecyclePhase::Finalize, MirHelperReference::Finalize(ty)),
            (LifecyclePhase::Destroy, MirHelperReference::Destroy(ty)),
            (
                LifecyclePhase::Cancel,
                MirHelperReference::Cleanup {
                    phase: MirCleanupPhase::TaskCancellation,
                    ty,
                },
            ),
        ] {
            if self.codegen_lifecycle_is_trivial(&helper, cancellation)? {
                continue;
            }

            let mut add = |call: LifecycleCallable| -> Result<(), CodegenPreparationError> {
                capacity += self.call_outgoing_capacity(
                    call.callable,
                    call.execution,
                    call.result,
                    cancellation,
                )?;

                Ok(())
            };

            match context.lifecycle_action(ty, phase)? {
                LifecycleAction::Call(call) => add(call)?,
                LifecycleAction::Owned {
                    borrow, teardown, ..
                } => {
                    add(borrow)?;

                    for call in teardown.into_iter().flatten() {
                        add(call)?;
                    }
                }
                // Structural forwarding uses the child owner's existing allowance.
                _ => {}
            }
        }

        Ok(capacity)
    }

    fn call_outgoing_capacity(
        &self,
        callable: CallableInstanceData,
        execution: CallableExecution,
        result: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<u32, CodegenPreparationError> {
        let unit = self.codegen_representation_type(RepresentationRole::Unit)?;

        if execution == CallableExecution::Synchronous
            && result == unit
            && let Some(body) = self.callable_body_key(callable.definition())?
        {
            let certified = self.certified_execution_with_cancellation(body, cancellation)?;

            // This measures a checked lifecycle invocation, not its authority to run.
            // Only the verified unconditional property applies to every legal receiver.
            // Guarded Total clauses remain dependent on their particular receiver state.
            if !certified.result().diagnostics().has_errors()
                && certified
                    .result()
                    .value()
                    .properties
                    .contains(&bray_checker::ExecutionProperty::Total)
            {
                return Ok(0);
            }
        }

        let empty = execution == CallableExecution::Synchronous
            && result == unit
            && self
                .verified_execution_obligation(
                    callable,
                    bray_checker::ExecutionObligation::Property(
                        bray_checker::ExecutionProperty::Total,
                        None,
                    ),
                    None,
                    cancellation,
                )?
                .is_some();

        Ok(u32::from(!empty))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lifecycle_total_certification_keeps_requirements_and_guards_distinct() {
        for (guarantee, body, total) in [
            ("executes(total)", "", true),
            (
                "when(self.ready) { executes(total) }",
                "if !self.ready { panic(\"not ready\"); }",
                false,
            ),
            ("executes(total)", "panic(\"failed\");", false),
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(owner: &Owner);
                struct Owner {{
                    ready: bool;
                    trusted destruct() requires(trusted live(owner = &self)) {guarantee} {{ {body} }}
                }}
                func forged() {{ let owner: Owner = {{ ready = true }}; }}
                func dispose(pos owner: Owner)
                    requires(trusted live(owner = &owner))
                {{}}
            "#
            );

            let compilation = crate::test_support::compilation(&source);

            let key = compilation
                .declared_unit_keys_for_test()
                .unwrap()
                .into_iter()
                .find(|key| {
                    key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
                        && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                })
                .unwrap();

            let certified = compilation.execution_properties(key).unwrap();

            assert_eq!(
                certified
                    .value()
                    .properties
                    .contains(&bray_checker::ExecutionProperty::Total),
                total,
                "{source}: {certified:#?}"
            );

            let disposal = compilation
                .lowered_unit(crate::test_support::source_function_body_key(
                    &compilation,
                    "dispose",
                ))
                .unwrap();

            assert!(!disposal.diagnostics().has_errors(), "{disposal:#?}");

            let ty = disposal
                .value()
                .as_ref()
                .unwrap()
                .mir()
                .unwrap()
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                .expect("authorized consumer retains its owner parameter")
                .ty();

            assert_eq!(
                compilation
                    .owner_outgoing_capacity(ty, &crate::CancellationToken::new())
                    .unwrap(),
                u32::from(!total)
            );

            let forged = compilation
                .lowered_unit(crate::test_support::source_function_body_key(
                    &compilation,
                    "forged",
                ))
                .unwrap();

            bray_testing::assert_goal_state_diagnostic_kind(
                forged.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    #[test]
    fn production_exit_moves_keep_the_existing_cleanup_allowance() {
        let compilation = crate::compilation::trusted_contracts::production_host_compilation(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            struct StoredExit { mut value: AttachmentExit; }
            func retain(pos selected: AttachmentExit) -> StoredExit
                requires(!selected.ready, trusted attachment_exit_inactive(owner = &selected))
                ensures(!result.value.ready, trusted attachment_exit_inactive(owner = &result.value))
            { return { value = selected }; }
            func take(pos stored: StoredExit) -> AttachmentExit
                requires(!stored.value.ready, trusted attachment_exit_inactive(owner = &stored.value))
                ensures(!result.ready, trusted attachment_exit_inactive(owner = &result))
            { return stored.value; }
            func mutate_then_take(pos mut stored: StoredExit) -> AttachmentExit
                requires(!stored.value.ready, trusted attachment_exit_inactive(owner = &stored.value))
                ensures(!result.ready, trusted attachment_exit_inactive(owner = &result))
            { stored.value.cleanup = none; return stored.value; }
        "#,
        );

        for (name, capacity) in [("retain", 1), ("take", 0)] {
            let lowered = compilation
                .lowered_unit(crate::test_support::source_function_body_key(
                    &compilation,
                    name,
                ))
                .expect("production exit transfer must lower");

            assert!(!lowered.diagnostics().has_errors(), "{name}: {lowered:#?}");

            let unit = lowered.value().as_ref().unwrap().mir().unwrap();

            let parameter = unit
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                .expect("transfer retains its owner parameter");

            assert_eq!(
                compilation
                    .owner_outgoing_capacity(parameter.ty(), &crate::CancellationToken::new())
                    .unwrap(),
                capacity,
                "only the exit owner's destructor admits a local allowance"
            );

            for block in unit.blocks() {
                for operation in block.operations() {
                    let operation = unit.operation(*operation).unwrap();

                    if let bray_ir::MirOperationKind::AdmitOutgoing { ty, .. } = operation.kind() {
                        assert_eq!(
                            compilation
                                .owner_outgoing_capacity(*ty, &crate::CancellationToken::new())
                                .unwrap(),
                            0,
                            "{name} must move the existing allowance"
                        );
                    }
                }
            }
        }

        let mutated = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "mutate_then_take",
            ))
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            mutated.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn retained_exit_read_transfers_its_allowance() {
        let declarations = "trusted internal module bray.runtime.bootstrap.host;";

        let compilation =
            crate::compilation::trusted_contracts::production_host_compilation(declarations);

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "take_attachment_exit",
            ))
            .unwrap();

        assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

        let unit = lowered.value().as_ref().unwrap().mir().unwrap();

        for block in unit.blocks() {
            for operation in block.operations() {
                if let bray_ir::MirOperationKind::AdmitOutgoing { ty, .. } =
                    unit.operation(*operation).unwrap().kind()
                {
                    assert_eq!(
                        compilation
                            .owner_outgoing_capacity(*ty, &crate::CancellationToken::new())
                            .unwrap(),
                        0,
                        "a raw move must transfer the stored owner's allowance"
                    );
                }
            }
        }

        for target in bray_target::NativeTarget::ALL {
            let compilation =
                crate::compilation::trusted_contracts::production_host_compilation_for_target(
                    declarations,
                    crate::SelectedTarget::for_native(target),
                );

            for name in [
                "select_cleanup_slot",
                "prepare_attachment_cleanup",
                "publish_attachment_cleanup",
                "store_attachment_cleanup",
                "thread_attachment_cleanup",
                "select_attachment_slot",
                "release_attachment_exit",
                "finish_attachment",
            ] {
                let lowered = compilation
                    .lowered_unit(crate::test_support::source_function_body_key(
                        &compilation,
                        name,
                    ))
                    .unwrap();

                assert!(
                    !lowered.diagnostics().has_errors(),
                    "{target:?}, {name}: {lowered:#?}"
                );

                if name != "prepare_attachment_cleanup" {
                    let unit = lowered.value().as_ref().unwrap().mir().unwrap();

                    for operation in unit
                        .blocks()
                        .iter()
                        .flat_map(|block| block.operations())
                        .map(|operation| unit.operation(*operation).unwrap())
                    {
                        if let bray_ir::MirOperationKind::AdmitOutgoing { ty, .. } =
                            operation.kind()
                        {
                            assert_eq!(
                                compilation
                                    .owner_outgoing_capacity(*ty, &crate::CancellationToken::new())
                                    .unwrap(),
                                0,
                                "{target:?}, {name} must use existing backing"
                            );
                        }
                    }
                }
            }
        }

        for (body, valid) in [
            (
                "let selected = trusted take_attachment_exit(lease, slot, incidents);",
                true,
            ),
            (
                "let copied: AttachmentSlot = { value = slot.value, storage = slot.storage }; let selected = trusted take_attachment_exit(lease, &copied, incidents);",
                false,
            ),
            (
                "let selected = trusted take_attachment_exit(other, slot, incidents);",
                false,
            ),
        ] {
            let caller = format!(
                r#"
                func probe(pos lease: &HostLease, pos other: &HostLease, pos slot: &AttachmentSlot, pos incidents: &mut u32)
                    requires(
                        trusted host_access_live(owner = lease),
                        trusted host_access_idle(owner = lease),
                        trusted host_access_live(owner = other),
                        trusted host_access_idle(owner = other),
                        trusted retained_attachment_exit(slot = slot, lease = lease),
                        trusted core.memory.valid_read<AttachmentExit>(pointer = slot.value, count = 1),
                        trusted core.memory.aligned_for<AttachmentExit>(pointer = slot.value),
                        trusted core.memory.initialized_as<AttachmentExit>(pointer = slot.value),
                        trusted core.memory.valid_write<(AttachmentExit?)>(pointer = slot.storage, count = 1),
                        trusted core.memory.aligned_for<(AttachmentExit?)>(pointer = slot.storage)
                    )
                {{ {body} }}
            "#
            );

            let compilation = crate::compilation::trusted_contracts::production_host_compilation(
                &format!("{declarations}\n{caller}"),
            );

            let lowered = compilation
                .lowered_unit(crate::test_support::source_function_body_key(
                    &compilation,
                    "probe",
                ))
                .unwrap();

            assert_eq!(
                !lowered.diagnostics().has_errors(),
                valid,
                "{body}: {lowered:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    lowered.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn host_formation_prepares_without_cleanup_admission() {
        let compilation =
            crate::compilation::trusted_contracts::production_host_compilation("module app;");

        for name in [
            "initialize_host",
            "prepare_host_storage",
            "empty_host_storage",
            "complete_host_formation",
            "shutdown_host",
            "take_host_storage",
            "release_host_storage",
            "access_prepared_host",
        ] {
            let lowered = compilation
                .lowered_unit(crate::test_support::source_function_body_key(
                    &compilation,
                    name,
                ))
                .unwrap();

            assert!(!lowered.diagnostics().has_errors(), "{name}: {lowered:#?}");

            let unit = lowered.value().as_ref().unwrap().mir().unwrap();

            for operation in unit
                .blocks()
                .iter()
                .flat_map(|block| block.operations())
                .map(|operation| unit.operation(*operation).unwrap())
            {
                if let bray_ir::MirOperationKind::AdmitOutgoing { ty, .. } = operation.kind() {
                    assert_eq!(
                        compilation
                            .owner_outgoing_capacity(*ty, &crate::CancellationToken::new())
                            .unwrap(),
                        0,
                        "{name} must prepare explicit backing without admission"
                    );
                }
            }
        }
    }

    #[test]
    fn production_host_lease_has_empty_outgoing_capacity() {
        let compilation = crate::compilation::trusted_contracts::production_host_compilation(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            func probe(pos lease: HostLease)
                requires(trusted host_access_live(owner = &lease), trusted attachment_cleanup_completed(owner = &lease))
            {}
        "#,
        );

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "probe",
            ))
            .expect("production lease consumer must lower");

        assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

        let ty = lowered
            .value()
            .as_ref()
            .unwrap()
            .mir()
            .unwrap()
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
            .expect("consumer must retain its lease parameter")
            .ty();

        assert_eq!(
            compilation
                .owner_outgoing_capacity(ty, &crate::CancellationToken::new())
                .expect("actual lease cleanup capacity must resolve"),
            0
        );
    }
}
