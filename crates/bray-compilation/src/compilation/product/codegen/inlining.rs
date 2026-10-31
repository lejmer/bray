use std::collections::BTreeMap;

use bray_bound_tree::BoundCallResult;
use bray_codegen::{
    CodegenCallSite, CodegenOptions, CodegenTarget, DebugInformationMode, OptimizationLevel,
    RuntimeObservationMode, SizePreference, demanded_callable_instance_for_call,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirCall, MirCallTarget, MirEdge, MirOperand, MirOperationId,
    MirOperationKind, MirStorageKind, MirTerminatorKind, MirUnaryOperator, MirUnit, MirUnitKey,
    MirUnitKind, inline_call, reconstruct_reachable,
};
use bray_symbols::{CallableAbi, TypeData, TypeId};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::{ConcreteCodegenCallee, ConcreteCodegenInstance};
use crate::fact::CancellationToken;

const MAX_INLINE_WORK: usize = 192;
const MAX_INLINE_DEPTH: usize = 4;

impl Compilation {
    pub(super) fn inline_concrete_mir(
        &self,
        owner: &ConcreteCodegenInstance,
        base: &MirUnit,
        options: CodegenOptions,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<MirUnit>, CodegenPreparationError> {
        if options.optimization() != OptimizationLevel::Full
            || options.debug_information() == DebugInformationMode::Full
            || options.runtime_observations() == RuntimeObservationMode::Memory
            || options.size_preference() == SizePreference::MinimumSize
            || !matches!(
                base.key(),
                MirUnitKey::Bound(_) | MirUnitKey::ImportedExecutable(_)
            )
            || !matches!(base.kind(), MirUnitKind::Synchronous)
            || base.frame_descriptor().is_some()
        {
            return Ok(None);
        }

        if !self
            .codegen_instance_signature(owner, cancellation)?
            .has_panic_report_context()
        {
            return Ok(None);
        }

        let mut stack = vec![owner.key().clone()];
        let mut remaining = MAX_INLINE_WORK;

        // MIR bodies own Arc-backed tables. The cached base remains immutable while expansion replaces this handle.
        let (body, changed) = self.expand_calls(
            owner,
            base.clone(),
            options,
            target,
            &mut stack,
            0,
            &mut remaining,
            cancellation,
        )?;

        Ok(changed.then_some(body))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "inlining carries one caller's policy, stack, and work budget"
    )]
    fn expand_calls(
        &self,
        owner: &ConcreteCodegenInstance,
        mut body: MirUnit,
        options: CodegenOptions,
        target: &CodegenTarget,
        stack: &mut Vec<bray_codegen::CodegenInstanceKey>,
        depth: usize,
        remaining: &mut usize,
        cancellation: &CancellationToken,
    ) -> Result<(MirUnit, bool), CodegenPreparationError> {
        let mut changed = false;
        let mut caller_types = BTreeMap::new();

        if depth >= MAX_INLINE_DEPTH || *remaining == 0 {
            return Ok((body, false));
        }

        loop {
            if *remaining == 0 {
                break;
            }

            let mut replacement = None;

            'sites: for (_, block) in body.blocks_with_ids() {
                for operation_id in block.operations() {
                    cancellation.check()?;

                    let operation = body.operation(*operation_id).expect("valid call site");

                    let MirOperationKind::Call(call) = operation.kind() else {
                        continue;
                    };

                    if call.arguments().len().max(1) > *remaining {
                        continue;
                    }

                    if options.size_preference() == SizePreference::Size
                        && (call.arguments().is_empty()
                            || call.arguments().iter().any(|argument| {
                                !matches!(
                                    argument.value(),
                                    MirOperand::Constant { .. } | MirOperand::Immediate { .. }
                                )
                            }))
                    {
                        continue;
                    }

                    let Some(callee) =
                        self.inline_callee(owner, *operation_id, call, target, cancellation)?
                    else {
                        continue;
                    };

                    if stack.contains(callee.key()) {
                        continue;
                    }

                    let raw = self.codegen_mir_for_plan(
                        callee.key(),
                        bray_ir::MirUnitId::new(0),
                        None,
                        cancellation,
                    )?;

                    let basic = options.with_optimization(OptimizationLevel::Basic);

                    let base =
                        self.optimized_mir_for_plan(&callee, &raw, basic, target, cancellation)?;

                    let cost = (base.operations().len().max(base.blocks().len())
                        + call.arguments().len())
                    .max(1);

                    if cost > *remaining {
                        continue;
                    }

                    let mut tentative = *remaining - cost;

                    stack.push(callee.key().clone());

                    let expanded = self.expand_calls(
                        &callee,
                        base,
                        options,
                        target,
                        stack,
                        depth + 1,
                        &mut tentative,
                        cancellation,
                    );

                    stack.pop();

                    let (expanded, nested_changed) = expanded?;

                    let expanded = if nested_changed {
                        let simplified =
                            self.simplify_concrete_mir(&callee, &expanded, cancellation)?;

                        reconstruct_reachable(&simplified)
                            .map_err(CodegenPreparationError::MirCapacity)?
                            .0
                    } else {
                        expanded
                    };

                    let Some((concrete_types, scalar_parameters)) =
                        self.inline_body_types(&callee, &expanded, cancellation)?
                    else {
                        continue;
                    };

                    let callables = if scalar_parameters {
                        BTreeMap::new()
                    } else {
                        self.concrete_mir_callables(&callee, &expanded, cancellation)?
                    };

                    for argument in call.arguments() {
                        let ty = body
                            .operand_type(argument.value())
                            .expect("checked call argument");

                        if !caller_types.contains_key(&ty) {
                            caller_types.insert(
                                ty,
                                self.concrete_codegen_type(
                                    ty,
                                    owner.substitution(),
                                    Some(owner),
                                    cancellation,
                                )?,
                            );
                        }
                    }

                    let Some(spliced) = inline_call(
                        &body,
                        *operation_id,
                        &expanded,
                        &concrete_types,
                        &callables,
                        &caller_types,
                        scalar_parameters,
                    )
                    .map_err(CodegenPreparationError::MirCapacity)?
                    else {
                        continue;
                    };

                    let inserted = spliced
                        .operations()
                        .len()
                        .saturating_sub(body.operations().len())
                        .max(spliced.blocks().len().saturating_sub(body.blocks().len()));

                    let extra = inserted.saturating_sub(*remaining - tentative);

                    if extra > tentative {
                        continue;
                    }

                    replacement = Some((spliced, tentative - extra));
                    break 'sites;
                }
            }

            let Some((spliced, next_remaining)) = replacement else {
                break;
            };

            body = spliced;
            *remaining = next_remaining;
            changed = true;
        }

        Ok((body, changed))
    }

    fn inline_callee(
        &self,
        owner: &ConcreteCodegenInstance,
        operation_id: MirOperationId,
        call: &MirCall,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCodegenInstance>, CodegenPreparationError> {
        if !matches!(call.target(), MirCallTarget::Direct(reference) if reference.abi() == CallableAbi::Bray)
            || !matches!(call.result(), BoundCallResult::Immediate(_))
            || call.is_cleanup()
            || call.intrinsic().is_some()
        {
            return Ok(None);
        }

        let demand =
            demanded_callable_instance_for_call(CodegenCallSite::Operation(operation_id), call)
                .expect("direct call has a concrete demand");

        let ConcreteCodegenCallee::Instance(callee) =
            self.concrete_codegen_callee(owner, &demand, None, target, cancellation)?
        else {
            return Ok(None);
        };

        if !matches!(
            callee.key().template(),
            MirUnitKey::Bound(_) | MirUnitKey::ImportedExecutable(_)
        ) {
            return Ok(None);
        }

        let signature = self.codegen_instance_signature(&callee, cancellation)?;

        if signature.abi() != CallableAbi::Bray
            || signature.is_variadic()
            || signature.parameters().len() != call.arguments().len()
        {
            return Ok(None);
        }

        Ok(Some(callee))
    }

    fn inline_body_types(
        &self,
        instance: &ConcreteCodegenInstance,
        body: &MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<Option<(BTreeMap<TypeId, TypeId>, bool)>, CodegenPreparationError> {
        // Scalar templates need only substitute operand types. Rich bodies also
        // retain the selected trait Self and their closed callable references.
        let values = self.semantic_value_store()?;

        let borrows_input = body.storages_with_ids().any(|(_, storage)| {
            matches!(storage.kind(), MirStorageKind::Parameter(_))
                && matches!(
                    values.type_data(storage.ty()).as_ref(),
                    TypeData::Borrow { .. }
                )
        });

        let has_bindings = instance.substitution().is_some_and(|substitution| {
            !values
                .generic_substitution_data(substitution)
                .bindings()
                .is_empty()
        });

        if !borrows_input || has_bindings {
            return self
                .scalar_body_is_eligible(instance, body, cancellation)
                .map(|types| types.map(|types| (types, true)));
        }

        let mut types = BTreeMap::new();

        for ty in body.referenced_types() {
            let concrete = self.concrete_codegen_type(
                ty,
                instance.substitution(),
                Some(instance),
                cancellation,
            )?;

            types.insert(ty, concrete);
        }

        if types.iter().any(|(source, target)| source != target)
            && !bray_codegen::demanded_constant_terms(body).is_empty()
        {
            return Ok(None);
        }

        if body
            .operations()
            .iter()
            .any(|operation| match operation.kind() {
                MirOperationKind::Call(call) => call.trait_dispatch().is_some(),
                MirOperationKind::Memory(memory) => matches!(
                    memory.kind(),
                    bray_bound_tree::CheckedMemoryOperationKind::InlineAssembly { .. }
                ),
                MirOperationKind::Async(_)
                | MirOperationKind::Host(_)
                | MirOperationKind::AnonymousCallable(_)
                | MirOperationKind::DeclaredCallable(_)
                | MirOperationKind::AdmitOutgoing { .. }
                | MirOperationKind::DischargeOutgoing { .. } => true,
                _ => false,
            })
        {
            return Ok(None);
        }

        Ok(Some((types, false)))
    }

    fn scalar_body_is_eligible(
        &self,
        instance: &ConcreteCodegenInstance,
        body: &MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<Option<BTreeMap<TypeId, TypeId>>, CodegenPreparationError> {
        let mut types = BTreeMap::new();

        for (_, storage) in body.storages_with_ids() {
            let Some(concrete) = self.concrete_scalar_type(storage.ty(), instance, cancellation)?
            else {
                return Ok(None);
            };

            if !matches!(
                storage.kind(),
                MirStorageKind::Parameter(_)
                    | MirStorageKind::Local
                    | MirStorageKind::Temporary
                    | MirStorageKind::Return
            ) {
                return Ok(None);
            }

            types.insert(storage.ty(), concrete);
        }

        for value in body.values() {
            let Some(concrete) = self.concrete_scalar_type(value.ty(), instance, cancellation)?
            else {
                return Ok(None);
            };

            types.insert(value.ty(), concrete);
        }

        for operation in body.operations() {
            let allowed = match operation.kind() {
                MirOperationKind::Store {
                    destination, value, ..
                } => {
                    destination.projections().is_empty()
                        && !matches!(
                            body.storage(destination.storage())
                                .expect("valid storage")
                                .kind(),
                            MirStorageKind::Parameter(_)
                        )
                        && self.scalar_operand(body, value, instance, &mut types, cancellation)?
                }
                MirOperationKind::Unary {
                    operator: MirUnaryOperator::Not | MirUnaryOperator::BitwiseNot,
                    operand,
                }
                | MirOperationKind::NumericConversion { operand, .. } => {
                    self.scalar_operand(body, operand, instance, &mut types, cancellation)?
                }
                MirOperationKind::Binary {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator,
                        MirBinaryOperator::Equal
                            | MirBinaryOperator::NotEqual
                            | MirBinaryOperator::LessThan
                            | MirBinaryOperator::LessThanOrEqual
                            | MirBinaryOperator::GreaterThan
                            | MirBinaryOperator::GreaterThanOrEqual
                            | MirBinaryOperator::BitwiseAnd
                            | MirBinaryOperator::BitwiseOr
                            | MirBinaryOperator::BitwiseXor
                    ) && self.scalar_operand(body, left, instance, &mut types, cancellation)?
                        && self.scalar_operand(body, right, instance, &mut types, cancellation)?
                }
                _ => false,
            };

            if !allowed {
                return Ok(None);
            }
        }

        for block in body.blocks() {
            let allowed = match block.terminator().kind() {
                MirTerminatorKind::Goto(edge) => {
                    self.scalar_edge(body, edge, instance, &mut types, cancellation)?
                }
                MirTerminatorKind::Return { value, report } => {
                    report.is_none()
                        && match value {
                            Some(value) => self.scalar_operand(
                                body,
                                value,
                                instance,
                                &mut types,
                                cancellation,
                            )?,
                            None => true,
                        }
                }
                MirTerminatorKind::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } => {
                    self.scalar_operand(body, condition, instance, &mut types, cancellation)?
                        && self.scalar_edge(body, then_edge, instance, &mut types, cancellation)?
                        && self.scalar_edge(body, else_edge, instance, &mut types, cancellation)?
                }
                MirTerminatorKind::Switch {
                    discriminant,
                    cases,
                    otherwise,
                } => {
                    let mut allowed = self.scalar_operand(
                        body,
                        discriminant,
                        instance,
                        &mut types,
                        cancellation,
                    )? && self.scalar_edge(
                        body,
                        otherwise,
                        instance,
                        &mut types,
                        cancellation,
                    )?;

                    for case in cases.iter() {
                        allowed &= self.scalar_edge(
                            body,
                            case.edge(),
                            instance,
                            &mut types,
                            cancellation,
                        )?;
                    }

                    allowed
                }
                _ => false,
            };

            if !allowed {
                return Ok(None);
            }
        }

        Ok(Some(types))
    }

    fn concrete_scalar_type(
        &self,
        ty: TypeId,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<Option<TypeId>, CodegenPreparationError> {
        let concrete = self.substitute_codegen_type(ty, instance.substitution(), cancellation)?;

        Ok(self.scalar_type(concrete)?.then_some(concrete))
    }

    fn scalar_edge(
        &self,
        body: &MirUnit,
        edge: &MirEdge,
        instance: &ConcreteCodegenInstance,
        types: &mut BTreeMap<TypeId, TypeId>,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        for argument in edge.arguments() {
            if !self.scalar_operand(body, argument, instance, types, cancellation)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn scalar_operand(
        &self,
        body: &MirUnit,
        operand: &MirOperand,
        instance: &ConcreteCodegenInstance,
        types: &mut BTreeMap<TypeId, TypeId>,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        let Some(ty) = body.operand_type(operand) else {
            return Ok(false);
        };

        let Some(concrete) = self.concrete_scalar_type(ty, instance, cancellation)? else {
            return Ok(false);
        };

        types.insert(ty, concrete);

        Ok(match operand {
            MirOperand::Value(_) | MirOperand::Constant { .. } | MirOperand::Immediate { .. } => {
                true
            }
            MirOperand::Copy(place) | MirOperand::Move(place) => place.projections().is_empty(),
            MirOperand::ConstantTerm { .. } => false,
        })
    }

    fn scalar_type(&self, ty: TypeId) -> Result<bool, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values.type_data(ty);

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(false);
        };

        let Some(role) =
            crate::compilation::foreign::compiler_known_representation(self, *definition)
        else {
            return Ok(false);
        };

        Ok(
            crate::compilation::representation::target_scalar(role).is_some()
                && !matches!(
                    role,
                    RepresentationRole::ScalarC32
                        | RepresentationRole::ScalarC64
                        | RepresentationRole::ScalarC128
                        | RepresentationRole::ScalarC256
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use bray_codegen::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};
    use bray_ir::{MirOperationKind, MirUnitKey, inline_call};
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
        PackageInterfaceIdentity, encode_package_interface,
    };
    use bray_symbols::PackageIdentity;

    use crate::fact::CancellationToken;

    fn reachability(
        source: &str,
        options: CodegenOptions,
    ) -> super::super::super::specialization::ConcreteCodegenReachability {
        let compilation = crate::test_support::compilation_with_product(
            source,
            bray_symbols::ProductKind::Executable,
        );

        reachability_for_compilation(&compilation, options)
    }

    fn reachability_for_compilation(
        compilation: &crate::Compilation,
        options: CodegenOptions,
    ) -> super::super::super::specialization::ConcreteCodegenReachability {
        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("test target");

        let semantic = compilation.product_semantics().expect("product semantics");

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .expect("root instances");

        compilation
            .codegen_reachability(roots, None, &target, options, false, &cancellation)
            .expect("reachability must close")
    }

    fn direct_calls(
        graph: &super::super::super::specialization::ConcreteCodegenReachability,
    ) -> usize {
        graph
            .graph()
            .instances()
            .iter()
            .map(|instance| {
                instance
                    .mir()
                    .operations()
                    .iter()
                    .filter(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
                    .count()
            })
            .sum()
    }

    #[test]
    fn full_inlines_scalar_wrapper_before_dependency_discovery() {
        let source = r#"
            module app;

            func unused() {}

            func enabled(pos value: bool) -> bool
            {
                return value;
            }

            func main()
            {
                if enabled(false)
                {
                    unused();
                }
            }
        "#;

        let compilation = crate::test_support::compilation_with_product(
            source,
            bray_symbols::ProductKind::Executable,
        );

        let release = crate::BuildConfiguration::Release.codegen_options();

        let full_debug = CodegenOptions::new(
            OptimizationLevel::Full,
            release.size_preference(),
            DebugInformationMode::Full,
            release.reproducibility(),
            release.runtime_observations(),
        );

        let none = reachability_for_compilation(&compilation, CodegenOptions::default());

        let basic = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let full = reachability_for_compilation(&compilation, release);

        let timed = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::TimedRelease {
                inner_iterations: std::num::NonZeroU64::new(3).expect("nonzero iterations"),
            }
            .codegen_options(),
        );

        let observed = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::ObservedRelease.codegen_options(),
        );

        let debug = reachability_for_compilation(&compilation, full_debug);

        assert_eq!(none.graph().instances().len(), 3);
        assert_eq!(basic.graph().instances().len(), 3);
        assert_eq!(full.graph().instances().len(), 1);
        assert_eq!(timed.graph().instances().len(), 1);
        assert_eq!(observed.graph().instances().len(), 3);
        assert_eq!(debug.graph().instances().len(), 3);
        assert!(direct_calls(&basic) > 0);
        assert_eq!(direct_calls(&full), 0);
        assert_eq!(direct_calls(&timed), 0);
        assert!(full.graph().instances()[0].mir().is_valid());
    }

    #[test]
    fn scalar_splices_keep_named_arguments_in_declaration_order() {
        let full = reachability(
            r"
            module app;
            func unused() {}
            func select(first: bool, second: bool) -> bool {
                return first && !second;
            }
            func main() {
                if select(second = false, first = true) {} else { unused(); }
            }
        ",
            crate::BuildConfiguration::Release.codegen_options(),
        );

        assert_eq!(full.graph().instances().len(), 1);
        assert_eq!(direct_calls(&full), 0);
    }

    #[test]
    fn nonunwinding_call_splice_keeps_following_operations() {
        let source = r#"
            module app;
            func unused() {}
            func identity(pos value: bool) -> bool { return value; }
            func main() {
                if !identity(true) { unused(); }
            }
        "#;

        let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

        assert_eq!(direct_calls(&full), 0);
        assert_eq!(full.graph().instances().len(), 1);
        assert!(full.graph().instances()[0].mir().is_valid());
    }

    #[test]
    fn recursive_calls_close_without_query_cycles() {
        let mutual = r#"
            module app;

            func first(pos value: bool) -> bool
            {
                if value { return second(false); }
                return false;
            }

            func second(pos value: bool) -> bool
            {
                if value { return first(false); }
                return false;
            }

            func main()
            {
                if first(false) {}
            }
        "#;

        let direct = r#"
            module app;

            func recur(pos value: bool) -> bool
            {
                if value { return recur(false); }
                return false;
            }

            func main()
            {
                if recur(false) {}
            }
        "#;

        for source in [direct, mutual] {
            let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

            assert!(
                full.graph()
                    .instances()
                    .iter()
                    .all(|instance| instance.mir().is_valid())
            );
        }
    }

    #[test]
    fn nested_scalar_wrappers_inline_into_the_final_body() {
        let source = r#"
            module app;

            func unused() {}

            func inner(pos value: bool) -> bool
            {
                return value;
            }

            func outer(pos value: bool) -> bool
            {
                return inner(value);
            }

            func main()
            {
                if outer(false) { unused(); }
            }
        "#;

        let basic = reachability(
            source,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

        assert_eq!(basic.graph().instances().len(), 4);
        assert_eq!(full.graph().instances().len(), 1);
        assert_eq!(direct_calls(&full), 0);
    }

    #[test]
    fn inline_budget_leaves_later_calls_and_is_repeatable() {
        let mut source = String::from(
            r#"
            module app;

            func unused() {}

            func enabled(pos value: bool) -> bool
            {
                let a = !value;
                let b = !a;
                let c = !b;
                let d = !c;
                let e = !d;
                let f = !e;
                let g = !f;
                let h = !g;
                return h;
            }

            func main()
            {
        "#,
        );

        for _ in 0..30 {
            source.push_str("if enabled(false) { unused(); }\n");
        }

        source.push_str("}\n");

        let compilation = crate::test_support::compilation_with_product(
            &source,
            bray_symbols::ProductKind::Executable,
        );

        let basic = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let first = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::Release.codegen_options(),
        );

        let second = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::Release.codegen_options(),
        );

        assert!(direct_calls(&first) > 0);
        assert!(direct_calls(&first) < direct_calls(&basic));
        assert_eq!(first.graph(), second.graph());
    }

    #[test]
    fn arithmetic_with_overflow_policy_keeps_the_call() {
        let source = r#"
            module app;

            func increment(pos value: i32) -> i32
            {
                return value + 1;
            }

            func main()
            {
                if increment(1) == 2 {}
            }
        "#;

        let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

        assert!(direct_calls(&full) > 0);

        assert!(
            full.graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn borrowed_arguments_inline_while_owned_cleanup_keeps_its_boundary() {
        let borrowed = r#"
            module app;

            func borrow_identity(pos value: &bool) -> &bool
            {
                return value;
            }

            func main()
            {
                let value: bool = false;
                let held: &bool = borrow_identity(&value);
            }
        "#;

        let owned = r#"
            module app;

            struct Flag
            {
                value: bool;

                destruct()
                {
                }
            }

            func inspect(pos value: Flag) -> bool
            {
                return value.value;
            }

            func main()
            {
                let value: Flag = Flag { value = false, };
                if inspect(value) {}
            }
        "#;

        let borrowed = reachability(
            borrowed,
            crate::BuildConfiguration::Release.codegen_options(),
        );

        assert_eq!(direct_calls(&borrowed), 0);

        let owned = reachability(owned, crate::BuildConfiguration::Release.codegen_options());

        assert!(direct_calls(&owned) > 0);
    }

    #[test]
    fn concrete_generic_scalar_wrapper_with_local_store_inlines() {
        let source = r#"
            module app;

            func unused() {}

            func identity<T>(pos value: T) -> T
            {
                let copy: T = value;
                return copy;
            }

            func main()
            {
                if identity<bool>(false)
                {
                    unused();
                }
            }
        "#;

        let basic = reachability(
            source,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

        assert!(direct_calls(&basic) > 0);
        assert_eq!(direct_calls(&full), 0);
        assert_eq!(full.graph().instances().len(), 1);

        assert!(
            full.graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn size_preferences_keep_repeated_dynamic_calls() {
        let source = r#"
            module app;

            func enabled(pos value: bool) -> bool
            {
                return value;
            }

            public func repeated(pos value: bool) -> bool
            {
                let first: bool = enabled(value);
                let second: bool = enabled(value);
                return first && second;
            }
        "#;

        let compilation = crate::test_support::compilation_with_product(
            source,
            bray_symbols::ProductKind::Library,
        );

        let release = crate::BuildConfiguration::Release.codegen_options();

        let options = |size| {
            CodegenOptions::new(
                release.optimization(),
                size,
                release.debug_information(),
                release.reproducibility(),
                release.runtime_observations(),
            )
        };

        let default = reachability_for_compilation(&compilation, options(SizePreference::None));
        let size = reachability_for_compilation(&compilation, options(SizePreference::Size));

        let minimum =
            reachability_for_compilation(&compilation, options(SizePreference::MinimumSize));

        assert_eq!(direct_calls(&default), 0);
        assert_eq!(direct_calls(&size), 2);
        assert_eq!(direct_calls(&minimum), 2);

        assert!(
            size.graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn async_boundary_does_not_inline() {
        let source = r#"
            module app;

            async func enabled(pos value: bool) -> bool
            {
                return value;
            }

            async func main()
            {
                if await enabled(false) {}
            }
        "#;

        let full = reachability(source, crate::BuildConfiguration::Release.codegen_options());

        assert!(direct_calls(&full) > 0);
    }

    #[test]
    fn scalar_splice_forwards_caller_values_through_a_branching_callee() {
        let compilation = crate::test_support::compilation_with_product(
            r#"
            module app;

            func select(pos value: bool) -> bool
            {
                if value
                {
                    return false;
                }
                return true;
            }

            func caller(pos value: bool) -> bool
            {
                return !value == select(value);
            }

            func main()
            {
                if caller(false) {}
            }
        "#,
            bray_symbols::ProductKind::Executable,
        );

        let graph = reachability_for_compilation(&compilation, CodegenOptions::default());

        let body = |name| {
            let key = crate::test_support::source_function_body_key(&compilation, name);

            graph
                .graph()
                .instances()
                .iter()
                .find(|instance| instance.mir().key() == &MirUnitKey::Bound(key.clone()))
                .expect("source function is reachable")
                .mir()
        };

        let caller = body("caller");
        let callee = body("select");

        assert!(
            callee.blocks().len() > 1,
            "regression covers callee control-flow forwarding"
        );

        assert!(
            caller.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Unary {
                    operator: bray_ir::MirUnaryOperator::Not,
                    ..
                }
            )),
            "caller retains the prefix value used after the call"
        );

        let (site, _) = caller
            .operations_with_ids()
            .find(|(_, operation)| matches!(operation.kind(), MirOperationKind::Call(_)))
            .expect("caller retains its selected call");

        let inlined = inline_call(
            caller,
            site,
            callee,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
        )
        .expect("MIR capacity")
        .expect("scalar call is eligible");

        assert!(inlined.is_valid());

        assert!(
            inlined
                .blocks()
                .iter()
                .any(|block| block.parameters().len() > 1),
            "callee blocks carry the caller's live prefix value in SSA"
        );
    }

    #[test]
    fn cross_file_splice_preserves_callee_source() {
        let compilation = crate::Compilation::load(crate::CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![
                crate::test_support::source_input(
                    "module app; func identity(pos value: bool) -> bool { return value; }",
                    0,
                ),
                crate::test_support::source_input(
                    "module app; func main() { if identity(false) {} }",
                    1,
                ),
            ],
            crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                bray_symbols::ProductKind::Executable,
                crate::SelectedTarget::baseline(),
            ),
        ))
        .expect("cross-file compilation");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let basic = reachability_for_compilation(
            &compilation,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let caller = basic
            .graph()
            .instances()
            .iter()
            .find(|instance| {
                instance
                    .mir()
                    .operations()
                    .iter()
                    .any(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
            })
            .expect("caller instance");

        let callee = basic
            .graph()
            .instances()
            .iter()
            .find(|instance| instance.key() != caller.key())
            .expect("callee instance");

        let (site, operation) = caller
            .mir()
            .operations_with_ids()
            .find(|(_, operation)| matches!(operation.kind(), MirOperationKind::Call(_)))
            .expect("call site");

        let inlined = inline_call(
            caller.mir(),
            site,
            callee.mir(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
        )
        .expect("MIR capacity")
        .expect("scalar splice");

        assert_ne!(operation.source(), callee.mir().blocks()[0].source());

        assert!(
            inlined
                .blocks()
                .iter()
                .any(|block| block.source() == callee.mir().blocks()[0].source())
        );

        assert!(inlined.is_valid());
    }

    fn imported_consumer(library_source: &str, consumer_source: &str) -> crate::Compilation {
        imported_consumer_with_native_inputs(library_source, consumer_source, &[])
    }

    fn imported_consumer_with_native_inputs(
        library_source: &str,
        consumer_source: &str,
        native_links: &[bray_symbols::NativeLinkRequirement],
    ) -> crate::Compilation {
        let package = PackageIdentity::try_new("example.dependency").expect("package identity");
        let product = InterfaceProductIdentity::try_new("library").expect("product identity");

        let identity = PackageInterfaceIdentity::try_new(
            package.clone(),
            crate::test_support::package_version(),
            product.clone(),
            InterfaceProductKind::Library,
            "public",
        )
        .expect("interface identity");

        let revision = InterfaceLanguageRevision::new(0);
        let export = crate::PackageInterfaceExportRequest::new(identity, revision);

        let library = crate::Compilation::load(
            crate::CompilationRequest::with_options(
                package.clone(),
                vec![crate::test_support::source_input(library_source, 0)],
                crate::CompilationOptions::new(
                    crate::WorkerBudget::serial(),
                    bray_symbols::ProductKind::Library,
                    crate::SelectedTarget::baseline(),
                )
                .with_native_link_inputs(native_links.iter().cloned()),
            )
            .with_package_interface_export(export)
            .with_dependency_interfaces([
                crate::test_support::runtime_standard_library_dependency(
                    &crate::SelectedTarget::baseline(),
                ),
            ]),
        )
        .expect("library compilation");

        assert!(
            library.check_diagnostics().is_empty(),
            "{:#?}",
            library.check_diagnostics()
        );

        let bundle = library
            .package_interface_export_bundle()
            .expect("export bundle")
            .as_ref()
            .expect("valid export");

        let interface = encode_package_interface(bundle).expect("encoded interface");
        let policy = InterfaceValidationPolicy::new(revision);

        let implementation = PackageImplementationArtifact::try_from_export_bundle(
            &interface,
            bundle,
            InterfaceValidationLimits::default(),
        )
        .expect("implementation artifact");

        let dependency = crate::DependencyInterfaceInput::new(
            package,
            product,
            "dependency.brayi",
            interface.shared_bytes(),
            policy,
        )
        .with_implementation_artifact("dependency.brayimpl", Arc::new(implementation));

        let consumer = crate::Compilation::load(
            crate::CompilationRequest::with_options(
                crate::test_support::package_identity(),
                vec![crate::test_support::source_input(consumer_source, 0)],
                crate::CompilationOptions::new(
                    crate::WorkerBudget::serial(),
                    bray_symbols::ProductKind::Executable,
                    crate::SelectedTarget::baseline(),
                )
                .with_native_link_inputs(native_links.iter().cloned()),
            )
            .with_dependency_interfaces([
                dependency,
                crate::test_support::runtime_standard_library_dependency(
                    &crate::SelectedTarget::baseline(),
                ),
            ]),
        )
        .expect("consumer compilation");

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{:#?}",
            consumer.check_diagnostics()
        );

        consumer
    }

    #[test]
    fn imported_transfer_leaves_keep_native_demand_separate_at_every_optimization_level() {
        use super::super::super::realization::NativeBoundaryMapping;

        let native_links = [bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("fixture").expect("fixture library name"),
            bray_symbols::NativeLinkKind::System,
        )];

        let library = r#"
            trusted module transfers;

            @link(name = "fixture", kind = system)
            @symbol(name = "fixture_read")
            @abi(c)
            extern trusted internal func native_read(pos length: i32) -> i32 uses(foreign_call);

            @link(name = "fixture", kind = system)
            @symbol(name = "fixture_write")
            @abi(c)
            extern trusted internal func native_write(pos length: i32) -> i32 uses(foreign_call);

            internal func request_length(pos length: i32) -> i32
            {
                return if length > 127 { yield 127; } else { yield length; };
            }

            internal func transfer_result(pos count: i32, progress: &mut i32) -> i32
            {
                if count < 0 { return -1; }
                progress = count;
                return 0;
            }

            trusted func read(pos length: i32, progress: &mut i32) -> i32
                uses(foreign_call)
            {
                progress = 0;
                if length == 0 { return 0; }
                let count: i32 = trusted native_read(request_length(length));
                return transfer_result(count, progress = progress);
            }

            trusted func write(pos length: i32, progress: &mut i32) -> i32
                uses(foreign_call)
            {
                progress = 0;
                if length == 0 { return 0; }
                let count: i32 = trusted native_write(request_length(length));
                return transfer_result(count, progress = progress);
            }
        "#;

        for operation in ["read", "write"] {
            let source = format!(
                "trusted module app; using example.dependency.transfers; \
                 func main() -> i32 {{ let mut progress: i32 = 0; \
                 return trusted example.dependency.transfers.{operation}(4, progress = &mut progress); }}"
            );

            let consumer = imported_consumer_with_native_inputs(library, &source, &native_links);

            for options in [
                CodegenOptions::default(),
                crate::BuildConfiguration::Development.codegen_options(),
                crate::BuildConfiguration::Release.codegen_options(),
            ] {
                let graph = reachability_for_compilation(&consumer, options);

                // Mutable progress keeps these imported bodies outside the scalar inliner's domain.
                assert!(graph.graph().instances().iter().any(|instance| {
                    matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
                }));

                assert!(
                    graph
                        .graph()
                        .instances()
                        .iter()
                        .all(|instance| instance.mir().is_valid())
                );

                let symbols = graph
                    .graph()
                    .external_instances()
                    .iter()
                    .filter_map(|instance| {
                        match consumer
                            .codegen_native_boundary(
                                graph
                                    .instance(instance)
                                    .expect("reachable concrete instance"),
                                &BTreeSet::new(),
                                &CancellationToken::new(),
                            )
                            .expect("native boundary")
                        {
                            Some(NativeBoundaryMapping::Direct { name, .. }) => Some(name),
                            _ => None,
                        }
                    })
                    .collect::<Vec<_>>();

                assert_eq!(symbols.len(), 1, "{symbols:?}");
                assert_eq!(symbols[0].as_str(), format!("fixture_{operation}"));
            }
        }
    }

    #[test]
    fn source_and_imported_static_borrowed_adapters_inline_before_outcome_lowering() {
        let adapter = r#"
            module adapters;
            public struct Sink { mut length: usize; }
            public struct Failure { detail: string; }
            func checked_length(pos count: usize) -> usize { assert(count != 0); return count; }
            func checked_count<T>(pos count: usize) -> Result<usize, Failure> {
                assert(count != 0);
                return Ok(count);
            }
            public trait Writer {
                mut func write(pos bytes: &[u8]) -> Result<usize, Failure>;
                mut func complete(pos bytes: &[u8]) -> usize {
                    let length: usize = match consume checked_count<Self>(bytes.length()) {
                        case Ok(count) { yield count; }
                        case Error(_) { panic("length failed"); }
                    };
                    let mut written: usize = 0;
                    while written < length {
                        let result: Result<usize, Failure> = self.write(&bytes[written..length]);
                        match result {
                            case Ok(count) {
                                if count == length - written { return length; }
                            }
                            case Error(_) {}
                        }
                        match consume result {
                            case Ok(count) {
                                assert(count > 0 && count <= length - written);
                                written += count;
                            }
                            case Error(error) { panic("write failed"); }
                        }
                    }
                    return length;
                }
            }
            impl SinkWriter = Sink(Writer) {
                mut func write(pos bytes: &[u8]) -> Result<usize, Failure> {
                    let count: usize = checked_length(bytes.length());
                    self.length = count;
                    return Ok(count);
                }
            }
        "#;

        let source = crate::Compilation::load(
            crate::CompilationRequest::with_options(
                crate::test_support::package_identity(),
                vec![crate::test_support::source_input(
                    &format!(
                        "{adapter} func main() {{ let mut sink: Sink = {{ length = 0 }}; let bytes: [u8; 4] = [1; 4]; assert(sink.complete(&bytes[..]) == 4); assert(sink.length == 4); }}"
                    ),
                    0,
                )],
                crate::CompilationOptions::new(
                    crate::WorkerBudget::serial(),
                    bray_symbols::ProductKind::Executable,
                    crate::SelectedTarget::baseline(),
                ),
            )
            .with_dependency_interfaces([
                crate::test_support::runtime_standard_library_dependency(
                    &crate::SelectedTarget::baseline(),
                ),
            ]),
        )
        .expect("source compilation");

        let imported = imported_consumer(
            adapter,
            r"
            module app;
            using example.dependency.adapters;
            using example.dependency.adapters.SinkWriter;
            func main() {
                let mut sink: example.dependency.adapters.Sink = { length = 0 };
                let bytes: [u8; 4] = [1; 4];
                assert(sink.complete(&bytes[..]) == 4);
                assert(sink.length == 4);
            }
        ",
        );

        for compilation in [&source, &imported] {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:?}",
                compilation.check_diagnostics()
            );

            for configuration in [
                crate::BuildConfiguration::Development,
                crate::BuildConfiguration::Release,
                crate::BuildConfiguration::TimedRelease {
                    inner_iterations: std::num::NonZeroU64::new(3).expect("nonzero iterations"),
                },
            ] {
                let graph =
                    reachability_for_compilation(compilation, configuration.codegen_options());

                let retained_adapter = graph.graph().instances().iter().any(|instance| {
                    match instance.key().template() {
                        MirUnitKey::Bound(key) => {
                            matches!(
                                key.declared_owner().kind(),
                                bray_symbols::SymbolKind::TraitCallableFulfillment
                                    | bray_symbols::SymbolKind::TraitCallableMember
                            )
                        }
                        MirUnitKey::ImportedExecutable(key) => {
                            matches!(
                                key.owner().kind(),
                                bray_symbols::SymbolKind::TraitCallableFulfillment
                                    | bray_symbols::SymbolKind::TraitCallableMember
                            )
                        }
                        _ => false,
                    }
                });

                assert_eq!(
                    retained_adapter,
                    configuration == crate::BuildConfiguration::Development
                );

                if configuration != crate::BuildConfiguration::Development {
                    let values = compilation.semantic_value_store().expect("semantic values");
                    let mut type_arguments = 0;

                    for instance in graph.graph().instances() {
                        let concrete = graph.instance(instance.key()).expect("concrete instance");

                        let Some(substitution) = concrete.substitution() else {
                            continue;
                        };

                        for binding in values.generic_substitution_data(substitution).bindings() {
                            if let bray_symbols::GenericArgument::Type(ty) = binding.argument() {
                                assert!(!matches!(
                                    values.type_data(ty).as_ref(),
                                    bray_symbols::TypeData::ContextualSelf(_)
                                ));

                                type_arguments += 1;
                            }
                        }
                    }

                    assert!(
                        type_arguments > 0,
                        "the generic helper remains a closed call"
                    );
                }

                assert!(
                    graph
                        .graph()
                        .instances()
                        .iter()
                        .all(|instance| instance.mir().is_valid())
                );
            }
        }
    }

    #[test]
    fn imported_scalar_template_inlines_at_concrete_call() {
        let consumer = imported_consumer(
            r#"
                module templates;

                public func identity(pos value: bool) -> bool
                {
                    return value;
                }
            "#,
            r#"
                module app;

                using example.dependency.templates.identity;

                func main()
                {
                    if example.dependency.templates.identity(false) {}
                }
            "#,
        );

        let basic = reachability_for_compilation(
            &consumer,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let full = reachability_for_compilation(
            &consumer,
            crate::BuildConfiguration::Release.codegen_options(),
        );

        assert!(basic.graph().instances().iter().any(|instance| {
            matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
        }));

        let imported = basic
            .graph()
            .instances()
            .iter()
            .find(|instance| matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)))
            .expect("imported MIR instance");

        let caller = basic
            .graph()
            .instances()
            .iter()
            .find(|instance| matches!(instance.key().template(), MirUnitKey::Bound(_)))
            .expect("source caller");

        let (site, operation) = caller
            .mir()
            .operations_with_ids()
            .find(|(_, operation)| matches!(operation.kind(), MirOperationKind::Call(_)))
            .expect("imported call site");

        let inlined = inline_call(
            caller.mir(),
            site,
            imported.mir(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
        )
        .expect("MIR capacity")
        .expect("scalar splice");

        assert!(inlined.is_valid());
        assert_ne!(imported.mir().blocks()[0].source(), operation.source());

        assert!(
            inlined
                .blocks()
                .iter()
                .any(|block| block.source() == imported.mir().blocks()[0].source())
        );

        assert_eq!(full.graph().instances().len(), 1);
        assert_eq!(direct_calls(&full), 0);
    }

    #[test]
    fn imported_concrete_generic_wrapper_with_local_store_inlines() {
        let consumer = imported_consumer(
            r#"
                module templates;

                public func identity<T>(pos value: T) -> T
                {
                    let copy: T = value;
                    return copy;
                }
            "#,
            r#"
                module app;

                using example.dependency.templates.identity;

                func main()
                {
                    if example.dependency.templates.identity<bool>(false) {}
                }
            "#,
        );

        let basic = reachability_for_compilation(
            &consumer,
            crate::BuildConfiguration::Development.codegen_options(),
        );

        let full = reachability_for_compilation(
            &consumer,
            crate::BuildConfiguration::Release.codegen_options(),
        );

        assert!(direct_calls(&basic) > 0);
        assert_eq!(direct_calls(&full), 0);
        assert_eq!(full.graph().instances().len(), 1);

        assert!(
            full.graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }
}
