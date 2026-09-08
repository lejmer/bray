use bray_codegen::CodegenParameterMapping;
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirUnit, MirUnitId};
use bray_lowering::TaskObservationMethod;
use bray_symbols::CallableDefinitionId;

use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn compiler_provided_task_method(
        &self,
        definition: CallableDefinitionId,
    ) -> Option<TaskObservationMethod> {
        match self
            .available_compiler_known_symbols()
            .symbol_implementation(definition.symbol())
        {
            Some(ImplementationHook::TaskJoin) => Some(TaskObservationMethod::Join),
            Some(ImplementationHook::TaskCancel) => Some(TaskObservationMethod::Cancel),
            _ => None,
        }
    }

    pub(in crate::compilation::product) fn codegen_compiler_provided_mir(
        &self,
        instance: &ConcreteCodegenInstance,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let definition = self.codegen_callable_definition(instance.key())?;

        let Some(method) = self.compiler_provided_task_method(definition) else {
            return self.codegen_heap_method_mir(instance, unit, cancellation);
        };

        let signature = self.codegen_instance_signature(instance, cancellation)?;

        let [CodegenParameterMapping::Direct { ty: task, .. }] = signature.parameters() else {
            return Err(ProductQueryFailure::Conflict {
                context: ProductQueryContext::CallableDefinition(definition),
                data: ProductDataKind::CallableParameters,
            }
            .into());
        };

        let context =
            super::synthetic::CompilationSyntheticLoweringContext::new(self, cancellation)?;

        bray_lowering::lower_task_observation(
            &context,
            unit,
            definition,
            method,
            *task,
            instance.key().target(),
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
    use bray_ir::{
        MirAsyncOperation, MirBlockKind, MirOperationKind, MirSuspensionKind, MirTerminatorKind,
        MirUnitId, MirUnitKey,
    };
    use bray_lowering::TaskObservationMethod;
    use bray_symbols::{CallableDefinitionId, TypeCallableMemberSymbolId};

    use crate::fact::CancellationToken;

    #[test]
    fn task_observers_own_distinct_normal_and_capture_cleanup_continuations() {
        let compilation = crate::test_support::compilation("module app; func main() {}");
        let cancellation = CancellationToken::new();

        let context = super::super::synthetic::CompilationSyntheticLoweringContext::new(
            &compilation,
            &cancellation,
        )
        .unwrap();

        let values = compilation.semantic_value_store().unwrap();
        let symbols = compilation.available_compiler_known_symbols();

        let completion = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let task = symbols
            .unary_representation_type(values, RepresentationRole::Task, completion)
            .unwrap()
            .unwrap();

        let selected = compilation.selected_target().target();

        let target =
            bray_ir::MirTargetContract::new(selected.profile().clone(), selected.runtime_abi());

        for (name, method) in [
            ("TaskJoin", TaskObservationMethod::Join),
            ("TaskCancel", TaskObservationMethod::Cancel),
        ] {
            let member = symbols
                .declaration_symbol::<TypeCallableMemberSymbolId>(
                    &CompilerKnownDeclarationKey::try_new(name).unwrap(),
                )
                .unwrap();

            let definition = CallableDefinitionId::try_new(member.into()).unwrap();

            assert_eq!(
                compilation.compiler_provided_task_method(definition),
                Some(method)
            );

            assert_eq!(
                compilation
                    .codegen_callable_template(definition, &cancellation)
                    .unwrap(),
                MirUnitKey::CompilerProvidedCallable(definition)
            );

            let mir = bray_lowering::lower_task_observation(
                &context,
                MirUnitId::new(83),
                definition,
                method,
                task,
                &target,
            )
            .unwrap();

            let frame = mir.frame_descriptor().unwrap();
            let entry = mir.block(mir.entry()).unwrap();

            assert!(matches!(
                entry.terminator().kind(),
                MirTerminatorKind::Suspend {
                    kind: MirSuspensionKind::TaskCompletion,
                    cancellation: Some(_),
                    ..
                }
            ));

            assert_eq!(
                entry
                    .operations()
                    .iter()
                    .filter(|id| matches!(
                        mir.operation(**id).unwrap().kind(),
                        MirOperationKind::Async(MirAsyncOperation::RequestTaskCancellation { .. })
                    ))
                    .count(),
                usize::from(method == TaskObservationMethod::Cancel)
            );

            assert_eq!(
                mir.block(frame.inactive_cleanup().unwrap()).unwrap().kind(),
                MirBlockKind::CleanupBroadcast
            );

            let ordinary = mir.reachable_blocks([mir.entry(), frame.inactive_cleanup().unwrap()]);

            let (quiescence, destruction) = frame.capture_abandonment().unwrap();

            let quiescent = mir.reachable_blocks([quiescence]);
            let destroyed = mir.reachable_blocks([destruction]);

            assert!(
                ordinary.is_disjoint(&quiescent)
                    && ordinary.is_disjoint(&destroyed)
                    && quiescent.is_disjoint(&destroyed)
            );

            assert_eq!(
                ordinary
                    .iter()
                    .filter(|id| matches!(
                        mir.block(**id).unwrap().terminator().kind(),
                        MirTerminatorKind::Suspend {
                            kind: MirSuspensionKind::TaskCompletion,
                            cancellation: None,
                            ..
                        }
                    ))
                    .count(),
                1
            );

            assert_eq!(
                quiescent
                    .iter()
                    .filter(|id| matches!(
                        mir.block(**id).unwrap().terminator().kind(),
                        MirTerminatorKind::Suspend {
                            kind: MirSuspensionKind::TaskCompletion,
                            cancellation: None,
                            ..
                        }
                    ))
                    .count(),
                1
            );

            assert!(destroyed.iter().all(|id| !matches!(
                mir.block(*id).unwrap().terminator().kind(),
                MirTerminatorKind::Suspend { .. }
            )));

            assert_eq!(
                mir.operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                            completion: None,
                            ..
                        })
                    ))
                    .count(),
                2
            );

            assert!(!mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(MirAsyncOperation::CreateFrame { .. })
            )));
        }
    }
}
