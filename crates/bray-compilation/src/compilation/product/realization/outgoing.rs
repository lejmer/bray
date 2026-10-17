use bray_codegen::{CodegenCleanupRequirement, CodegenTarget};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirCallTarget, MirCleanupPhase, MirHelperReference, MirOperationKind};
use bray_symbols::{CallableExecution, CallableInstanceData, TypeId};

use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn operation_outgoing_requirements(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<Vec<CodegenCleanupRequirement>>, CodegenPreparationError> {
        match operation {
            MirOperationKind::AdmitOutgoing {
                owner: bray_ir::MirOutgoingOwner::Value(ty),
                ..
            }
            | MirOperationKind::DischargeOutgoing {
                owner: bray_ir::MirOutgoingOwner::Value(ty),
                ..
            } => {
                let ty = self.concrete_codegen_type(
                    *ty,
                    owner.substitution(),
                    Some(owner),
                    cancellation,
                )?;

                self.owner_outgoing_requirements(ty, cancellation).map(Some)
            }
            MirOperationKind::AdmitOutgoing {
                owner: bray_ir::MirOutgoingOwner::Invocation { callable, result },
                ..
            }
            | MirOperationKind::DischargeOutgoing {
                owner: bray_ir::MirOutgoingOwner::Invocation { callable, result },
                ..
            } => self
                .invocation_outgoing_requirements(owner, *callable, *result, target, cancellation)
                .map(Some),
            MirOperationKind::Call(call) if call.is_cleanup() => {
                let MirCallTarget::Direct(reference) = call.target() else {
                    panic!(
                        "checked cleanup call in {:?} must retain its selected direct target, got {:?}",
                        owner.key(),
                        call.target()
                    );
                };

                self.invocation_outgoing_requirements(
                    owner,
                    *reference,
                    call.result(),
                    target,
                    cancellation,
                )
                .map(Some)
            }
            _ => Ok(None),
        }
    }

    fn invocation_outgoing_requirements(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: bray_ir::MirCallableReference,
        result: bray_bound_tree::BoundCallResult,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCleanupRequirement>, CodegenPreparationError> {
        let callee = self.concrete_codegen_callable_data(
            owner,
            &reference.instance(),
            target,
            cancellation,
        )?;

        let callable = callee
            .callable_instance()
            .expect("selected invocation has a concrete callable instance");

        let (execution, result) = match result {
            bray_bound_tree::BoundCallResult::Immediate(ty) => (CallableExecution::Synchronous, ty),
            bray_bound_tree::BoundCallResult::LazyFuture(future) => {
                (CallableExecution::Asynchronous, future.completion_type())
            }
        };

        let result =
            self.concrete_codegen_type(result, owner.substitution(), Some(owner), cancellation)?;

        if !self.call_requires_outgoing(callable, execution, result, cancellation)? {
            return Ok(Vec::new());
        }

        Ok(vec![outgoing_requirement((callee, result, execution))])
    }

    pub(super) fn owner_outgoing_requirements(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCleanupRequirement>, CodegenPreparationError> {
        Ok(self
            .owner_outgoing_calls(ty, cancellation)?
            .into_iter()
            .map(outgoing_requirement)
            .collect())
    }

    pub(super) fn owner_outgoing_calls(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<(ConcreteCodegenInstance, TypeId, CallableExecution)>, CodegenPreparationError>
    {
        let target = self
            .selected_target()
            .target()
            .codegen_target()
            .expect("selected target must support concrete cleanup realization");

        let mut requirements = Vec::new();

        for helper in [
            MirHelperReference::Finalize(ty),
            MirHelperReference::Destroy(ty),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty,
            },
        ] {
            if self.codegen_lifecycle_is_trivial(&helper, cancellation)? {
                continue;
            }

            let instance = self.concrete_codegen_lifecycle(helper.clone(), &target)?;

            let action = self.codegen_lifecycle_action(&helper, cancellation)?;

            for call in action.local_calls() {
                if self.call_requires_outgoing(
                    call.callable,
                    call.execution,
                    call.result,
                    cancellation,
                )? {
                    let callee = self.concrete_codegen_callable_data(
                        &instance,
                        &call.callable,
                        &target,
                        cancellation,
                    )?;

                    requirements.push((callee, call.result, call.execution));
                }
            }
        }

        Ok(requirements)
    }

    fn call_requires_outgoing(
        &self,
        callable: CallableInstanceData,
        execution: CallableExecution,
        result: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        let unit = self.codegen_representation_type(RepresentationRole::Unit)?;

        // Only a certificate covering every legal input can remove backing.
        Ok(!(execution == CallableExecution::Synchronous
            && result == unit
            && self.certified_entry_property(
                callable,
                bray_checker::ExecutionProperty::Total,
                cancellation,
            )?))
    }

    #[cfg(test)]
    pub(super) fn owner_outgoing_capacity(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<u32, CodegenPreparationError> {
        Ok(
            u32::try_from(self.owner_outgoing_requirements(ty, cancellation)?.len())
                .expect("local cleanup requirement count must fit its native descriptor"),
        )
    }
}

pub(super) fn outgoing_requirement(
    (callee, result, execution): (ConcreteCodegenInstance, TypeId, CallableExecution),
) -> CodegenCleanupRequirement {
    CodegenCleanupRequirement::new(
        super::names::generated_identity("cleanup_storage", callee.key()),
        callee.key().clone(),
        result,
        execution,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn async_cleanup_admission_does_not_generate_lifecycle_bodies() {
        use crate::fact::{CompilationFactKey, FactEvaluationTestObserver};

        let compilation = crate::test_support::compilation(
            r#"
module app;
struct Owner {
    async finalize() { panic("cleanup"); }
}
async func abandon(pos owner: Owner) { panic("body"); }
"#,
        );

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "abandon",
            ))
            .unwrap();

        assert!(!lowered.diagnostics().has_errors(), "{lowered:?}");

        let owner = lowered
            .value()
            .as_ref()
            .unwrap()
            .mir()
            .unwrap()
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
            .unwrap()
            .ty();

        compilation
            .state
            .fact_runtime
            .set_test_observer(FactEvaluationTestObserver::new(|key| {
                assert!(
                    !matches!(key, CompilationFactKey::GeneratedLifecycle(..)),
                    "admission consumes selected actions without generating execution: {key:?}"
                );
            }))
            .unwrap();

        let calls = compilation
            .owner_outgoing_calls(owner, &crate::CancellationToken::new())
            .unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, bray_symbols::CallableExecution::Asynchronous);
    }

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
            func extract_nominal_witness(pos stored: StoredExit) -> AttachmentExit
                requires(!stored.value.ready, trusted attachment_exit_inactive(owner = &stored.value))
                ensures(!result.ready, trusted attachment_exit_inactive(owner = &result))
            { return stored.value; }
            func retain(pos selected: AttachmentExit) -> (AttachmentExit, unit)
                requires(!selected.ready, trusted attachment_exit_inactive(owner = &selected))
                ensures(!result.0.ready, trusted attachment_exit_inactive(owner = &result.0))
            { return (selected, unit); }
            func take(pos stored: (AttachmentExit, unit)) -> AttachmentExit
                requires(!stored.0.ready, trusted attachment_exit_inactive(owner = &stored.0))
                ensures(!result.ready, trusted attachment_exit_inactive(owner = &result))
            { return stored.0; }
            func mutate_then_take(pos mut stored: (AttachmentExit, unit)) -> AttachmentExit
                requires(!stored.0.ready, trusted attachment_exit_inactive(owner = &stored.0))
                ensures(!result.ready, trusted attachment_exit_inactive(owner = &result))
            { stored.0.cleanup = none; return stored.0; }
        "#,
        );

        let nominal = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "extract_nominal_witness",
            ))
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            nominal.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
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

                    if let bray_ir::MirOperationKind::AdmitOutgoing {
                        owner: bray_ir::MirOutgoingOwner::Value(ty),
                        ..
                    } = operation.kind()
                    {
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
                if let bray_ir::MirOperationKind::AdmitOutgoing {
                    owner: bray_ir::MirOutgoingOwner::Value(ty),
                    ..
                } = unit.operation(*operation).unwrap().kind()
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
                        if let bray_ir::MirOperationKind::AdmitOutgoing {
                            owner: bray_ir::MirOutgoingOwner::Value(ty),
                            ..
                        } = operation.kind()
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
                        trusted host_access_live(owner = &lease.acquisition),
                        trusted host_access_idle(owner = lease),
                        trusted host_access_live(owner = &other.acquisition),
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

        for name in ["initialize_host", "shutdown_host", "access_prepared_host"] {
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
                if let bray_ir::MirOperationKind::AdmitOutgoing {
                    owner: bray_ir::MirOutgoingOwner::Value(ty),
                    ..
                } = operation.kind()
                {
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
                requires(trusted host_access_live(owner = &lease.acquisition), trusted attachment_cleanup_completed(owner = &lease))
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
