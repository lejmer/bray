use super::super::super::call::ConstantTemplateResolver;
use super::super::super::limits::{ConstantEvaluationLimits, EvaluationBudget};
use super::super::support::{
    TemplateEvaluationFailure, check_definition_materialization, template_index,
};
use crate::{CheckerQueryError, CheckerRequestContext};
use bray_bound_tree::{
    CheckedTemplate, CheckedTemplateKind, CheckedTemplateNodeId, CheckedTemplateOperation,
    CheckedTemplateShortCircuitKind,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    ConstantValueData, ConstantValueId, ConstantValueKind, GenericSubstitutionId, TypeId,
};
use std::collections::BTreeSet;

pub(in crate::constant::template_evaluation) struct TemplateEvaluator<'evaluation, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::constant::template_evaluation) context: &'evaluation C,
    pub(in crate::constant::template_evaluation) template: Option<&'evaluation CheckedTemplate>,
    pub(in crate::constant::template_evaluation) substitution: Option<GenericSubstitutionId>,
    pub(in crate::constant::template_evaluation) selected_implementation:
        Option<bray_symbols::ImplementationInstanceId>,
    pub(in crate::constant::template_evaluation) arguments: &'evaluation [ConstantValueId],
    pub(in crate::constant::template_evaluation) resolver:
        &'evaluation dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    pub(in crate::constant::template_evaluation) limits: ConstantEvaluationLimits,
    pub(in crate::constant::template_evaluation) budget: &'evaluation mut EvaluationBudget,
    pub(in crate::constant::template_evaluation) values: Vec<Option<ConstantValueId>>,
    types: crate::TypeNormalizer<'evaluation, C>,
    pub(super) generic_inputs: Vec<Option<ConstantValueId>>,
    pub(in crate::constant::template_evaluation) outputs:
        std::collections::BTreeMap<CheckedTemplateNodeId, Vec<Option<ConstantValueId>>>,
    pub(in crate::constant::template_evaluation) checked_materialization: BTreeSet<ConstantValueId>,
    pub(in crate::constant::template_evaluation) diagnostics: DiagnosticBag,
    pub(in crate::constant::template_evaluation) upstream_failure: Option<C::UpstreamError>,
    pub(in crate::constant::template_evaluation) static_initializer: bool,
}

impl<'evaluation, C> TemplateEvaluator<'evaluation, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::constant::template_evaluation) fn new(
        context: &'evaluation C,
        template: Option<&'evaluation CheckedTemplate>,
        substitution: Option<GenericSubstitutionId>,
        resolver: &'evaluation dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
        limits: ConstantEvaluationLimits,
        budget: &'evaluation mut EvaluationBudget,
    ) -> Self {
        Self {
            context,
            template,
            substitution,
            selected_implementation: None,
            arguments: &[],
            resolver,
            limits,
            budget,
            values: vec![None; template.map_or(0, |template| template.nodes().len())],
            types: crate::TypeNormalizer::new(context, substitution),
            generic_inputs: Vec::new(),
            outputs: Default::default(),
            checked_materialization: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
            upstream_failure: None,
            static_initializer: false,
        }
    }

    pub(in crate::constant::template_evaluation) fn template(
        &self,
    ) -> &'evaluation CheckedTemplate {
        self.template
            .expect("node evaluation retains its checked template")
    }

    pub(in crate::constant::template_evaluation) fn instantiate_type(
        &mut self,
        ty: TypeId,
    ) -> Result<TypeId, TemplateEvaluationFailure> {
        let ty = self.substitution.map_or(ty, |substitution| {
            self.context
                .semantic_values()
                .substitute_type(ty, substitution)
                .expect("checked evaluation retains a valid type substitution")
        });

        let normalized = self.types.normalize_type(ty);

        self.diagnostics.extend(self.types.take_diagnostics());

        let ty = normalized.map_err(|error| self.record_query_failure(error))?;

        if self.diagnostics.has_errors() {
            return Err(TemplateEvaluationFailure::Diagnosed);
        }

        Ok(ty)
    }

    pub(in crate::constant::template_evaluation) fn substitute_term(
        &self,
        term: bray_symbols::ConstantTermId,
    ) -> bray_symbols::ConstantTermId {
        self.substitution.map_or(term, |substitution| {
            self.context
                .semantic_values()
                .substitute_constant_term(term, substitution)
                .expect("checked evaluation retains a valid constant substitution")
        })
    }

    pub(in crate::constant::template_evaluation) fn substitute_arguments(
        &self,
        arguments: GenericSubstitutionId,
    ) -> GenericSubstitutionId {
        self.substitution.map_or(arguments, |substitution| {
            self.context
                .semantic_values()
                .substitute_generic_substitution(arguments, substitution)
                .expect("checked evaluation retains valid generic arguments")
        })
    }

    pub(in crate::constant::template_evaluation) fn evaluate_generic_inputs(
        &mut self,
    ) -> Result<(), TemplateEvaluationFailure> {
        for (index, input) in self.template().inputs().iter().enumerate() {
            if matches!(
                input.kind(),
                bray_bound_tree::CheckedTemplateInputKind::GenericConstant(_)
            ) {
                self.evaluate_input(bray_bound_tree::CheckedTemplateInputId::new(
                    u32::try_from(index).expect("checked input ordinal fits its template"),
                ))?;
            }
        }

        Ok(())
    }

    pub(in crate::constant::template_evaluation) fn evaluate_result(
        &mut self,
        kind: CheckedTemplateKind,
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if self.template().kind() != kind {
            panic!("constant template must match requested kind {kind:?}");
        }

        self.static_initializer = matches!(
            kind,
            CheckedTemplateKind::ProductStaticInitializer
                | CheckedTemplateKind::ThreadLocalStaticInitializer
        );

        let value = self.evaluate_node(self.template().result())?;
        let data = self.context.semantic_values().constant_value_data(value);

        if !matches!(
            kind,
            CheckedTemplateKind::ConstantCallableBody | CheckedTemplateKind::RuntimeDefault
        ) {
            check_definition_materialization(self, value)?;
        }

        if data.ty() != result_type {
            panic!("constant template result must have type {result_type:?}, actual: {data:?}");
        }

        Ok(value)
    }

    pub(in crate::constant::template_evaluation) fn evaluate_node(
        &mut self,
        node: CheckedTemplateNodeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        self.observe_cancellation()?;

        let index = template_index(node.raw());

        if let Some(value) = self.values.get(index).copied().flatten() {
            return Ok(value);
        }

        self.budget
            .try_charge_step()
            .map_err(TemplateEvaluationFailure::Diagnostic)?;

        let node = self.template().nodes().get(index).unwrap_or_else(|| {
            panic!(
                "evaluate_node requires constant template node, node: {node:?}, index: {index:?}"
            )
        });

        let ty = self.instantiate_type(node.ty())?;

        let value = self.evaluate_operation(
            CheckedTemplateNodeId::new(u32::try_from(index).expect("checked node fits its ID")),
            node.operation(),
            ty,
        )?;

        if self.diagnostics.has_errors() {
            return Err(TemplateEvaluationFailure::Diagnosed);
        }

        let slot = self
            .values
            .get_mut(index).unwrap_or_else(|| panic!("evaluate_node requires constant template value slot, node: {node:?}, index: {index:?}"));

        *slot = Some(value);

        Ok(value)
    }

    pub(super) fn evaluate_operation(
        &mut self,
        owner: CheckedTemplateNodeId,
        operation: &CheckedTemplateOperation,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        match operation {
            CheckedTemplateOperation::LoopValue { .. } => {
                panic!("checked loop state is read only under its kernel binding")
            }
            operation @ CheckedTemplateOperation::Loop { .. } => {
                self.evaluate_loop(owner, operation)
            }
            CheckedTemplateOperation::Unreachable => panic!(
                "constant evaluation reached a path excluded by checked control flow and invocation requirements"
            ),
            CheckedTemplateOperation::Sequence { before, value } => {
                self.evaluate_node(*before)?;

                self.evaluate_node(*value)
            }
            CheckedTemplateOperation::Snapshot { value } => self.evaluate_node(*value),
            CheckedTemplateOperation::StateAfter { operation, ordinal } => {
                self.evaluate_node(*operation)?;

                Ok(self.outputs[operation]
                    [usize::try_from(*ordinal).expect("checked state ordinal fits host")]
                .expect("an owner state is read only on a path where it is initialized"))
            }
            CheckedTemplateOperation::Update {
                subject,
                base,
                component,
                value,
            } => {
                self.evaluate_node(*base)?;
                self.evaluate_node(*component)?;

                let value = self.evaluate_node(*value)?;
                let subject = self.evaluate_node(*subject)?;

                self.replace_component(subject, *base, *component, value)
            }
            CheckedTemplateOperation::Input(input) => self.evaluate_input(*input),
            CheckedTemplateOperation::Constant { term, usage } => {
                let usage = crate::ConstantEvaluationUsage::new(
                    0,
                    usage.aggregate_elements(),
                    usage.literal_bytes(),
                )
                .with_expansions(usage.expansions());

                self.budget
                    .try_charge_usage(usage)
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                let term = self.substitute_term(*term);

                self.evaluate_term(term, ty)
            }
            CheckedTemplateOperation::Unary { operation, operand } => {
                let operand = self.evaluate_node(*operand)?;

                self.evaluate_unary(*operation, operand, ty)
            }
            CheckedTemplateOperation::Binary {
                operation,
                left,
                right,
            } => self.evaluate_binary(*operation, *left, *right, ty, false),
            CheckedTemplateOperation::MachineBinary {
                operation,
                left,
                right,
            } => self.evaluate_binary(*operation, *left, *right, ty, true),
            CheckedTemplateOperation::MachineUnary { operation, operand } => {
                let operand = self.evaluate_node(*operand)?;

                self.evaluate_machine_unary(*operation, operand, ty)
            }
            CheckedTemplateOperation::Borrow { kind, operand } => {
                self.evaluate_borrow(*kind, *operand, ty)
            }
            CheckedTemplateOperation::Declaration { declaration, .. } => {
                self.evaluate_declaration(declaration, ty)
            }
            CheckedTemplateOperation::Call {
                callable,
                substitution,
                arguments,
                ordinals,
                state_inputs,
                dispatch,
            } => self.evaluate_call(
                owner,
                state_inputs.as_deref(),
                callable,
                *substitution,
                arguments,
                ordinals,
                dispatch.as_ref(),
                ty,
            ),
            CheckedTemplateOperation::Convert { value, target } => {
                let value = self.evaluate_node(*value)?;
                let target = self.instantiate_type(*target)?;

                self.evaluate_conversion(value, target)
            }
            CheckedTemplateOperation::ArrayElement { .. } => {
                panic!("checked array element is evaluated only under its transform binding")
            }
            CheckedTemplateOperation::ArrayTransform {
                subject,
                element,
                value,
            } => self.evaluate_array_transform(*subject, *element, *value, ty),
            CheckedTemplateOperation::Tuple(elements) => {
                let values = self.evaluate_nodes(elements)?;

                self.budget
                    .try_charge_elements(values.len())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                Ok(self.intern_value(ty, ConstantValueKind::Tuple(values.into())))
            }
            CheckedTemplateOperation::Array(elements) => {
                let values = self.evaluate_nodes(elements)?;

                self.budget
                    .try_charge_elements(values.len())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                Ok(self.intern_value(ty, ConstantValueKind::Array(values.into())))
            }
            CheckedTemplateOperation::Construct { variant, fields } => {
                self.evaluate_construction(variant.as_ref(), fields, ty)
            }
            CheckedTemplateOperation::VariantTest { subject, variant } => {
                self.evaluate_variant_test(*subject, variant, ty)
            }
            CheckedTemplateOperation::NullableValue { subject } => {
                let subject = self.evaluate_node(*subject)?;
                let subject = self.observed_value(subject)?;

                let ConstantValueKind::NullablePresent(value) = subject.kind() else {
                    panic!("checked nullable projection observes a present value");
                };

                Ok(*value)
            }
            CheckedTemplateOperation::TupleElement { subject, index } => {
                self.evaluate_tuple_element(*subject, *index)
            }
            CheckedTemplateOperation::Project { subject, member } => {
                let subject = self.evaluate_node(*subject)?;

                self.evaluate_projection(subject, member, ty)
            }
            CheckedTemplateOperation::Index { call: Some(_), .. }
            | CheckedTemplateOperation::Slice { call: Some(_), .. } => {
                // Custom indexing reaches borrowed runtime storage, which cannot be a constant value.
                Err(TemplateEvaluationFailure::invalid_expression(
                    bray_diagnostics::DiagnosticExpressionCategory::Indexing,
                ))
            }
            CheckedTemplateOperation::Index {
                subject,
                index,
                call: None,
            } => {
                let subject = self.evaluate_node(*subject)?;
                let index = self.evaluate_index_bound(*index)?;
                let subject = self.observed_value(subject)?;

                let ConstantValueKind::Array(elements) = subject.kind() else {
                    panic!("constant index operation must receive an array, actual: {subject:?}");
                };

                elements.get(index).copied().ok_or_else(|| {
                    TemplateEvaluationFailure::invalid_expression(
                        bray_diagnostics::DiagnosticExpressionCategory::Indexing,
                    )
                })
            }
            CheckedTemplateOperation::Slice {
                subject,
                lower,
                upper,
                call: None,
            } => self.evaluate_slice(*subject, *lower, *upper, ty),
            CheckedTemplateOperation::Conditional {
                condition,
                when_true,
                when_false,
                state_outputs,
            } => {
                let condition = self.evaluate_node(*condition)?;

                let selected = self.boolean(condition)?;
                let value = self.evaluate_node(if selected { *when_true } else { *when_false })?;
                let mut outputs = Vec::with_capacity(state_outputs.len());

                for (left, right) in state_outputs.iter() {
                    outputs.push(if let Some(state) = if selected { left } else { right } {
                        Some(self.evaluate_node(*state)?)
                    } else {
                        None
                    });
                }

                self.outputs.insert(owner, outputs);

                Ok(value)
            }
            CheckedTemplateOperation::ShortCircuit { kind, left, right } => {
                let left = self.evaluate_node(*left)?;
                let left_value = self.boolean(left)?;

                match (kind, left_value) {
                    (CheckedTemplateShortCircuitKind::And, false)
                    | (CheckedTemplateShortCircuitKind::Or, true) => Ok(left),
                    _ => self.evaluate_node(*right),
                }
            }
            CheckedTemplateOperation::Temporary(temporary) => {
                let index = template_index(temporary.raw());

                let temporary = self
                    .template()
                    .temporaries()
                    .get(index).unwrap_or_else(|| panic!("evaluate_operation requires constant template temporary, ty: {ty:?}, index: {index:?}"));

                self.evaluate_node(temporary.initializer())
            }
        }
    }

    pub(in crate::constant::template_evaluation) fn evaluate_nodes(
        &mut self,
        nodes: &[CheckedTemplateNodeId],
    ) -> Result<Vec<ConstantValueId>, TemplateEvaluationFailure> {
        nodes.iter().map(|node| self.evaluate_node(*node)).collect()
    }

    pub(in crate::constant::template_evaluation) fn boolean(
        &self,
        value: ConstantValueId,
    ) -> Result<bool, TemplateEvaluationFailure> {
        let value = self.context.semantic_values().constant_value_data(value);

        let ConstantValueKind::Boolean(value) = value.kind() else {
            panic!("checked Boolean constant must be a Boolean value, actual: {value:?}");
        };

        Ok(*value)
    }

    pub(in crate::constant::template_evaluation) fn intern_value(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> ConstantValueId {
        self.context
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))
            .unwrap_or_else(|error| {
                panic!("intern_value must satisfy its checked construction contract: {error:?}")
            })
    }

    pub(in crate::constant::template_evaluation) fn observe_cancellation(
        &self,
    ) -> Result<(), TemplateEvaluationFailure> {
        if self.context.cancellation().is_cancelled() {
            Err(TemplateEvaluationFailure::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(in crate::constant::template_evaluation) fn record_query_failure(
        &mut self,
        error: CheckerQueryError<C::UpstreamError>,
    ) -> TemplateEvaluationFailure {
        match error {
            CheckerQueryError::Cancelled => TemplateEvaluationFailure::Cancelled,
            CheckerQueryError::Upstream(error) => {
                self.upstream_failure = Some(error);

                TemplateEvaluationFailure::Upstream
            }
        }
    }

    pub(in crate::constant::template_evaluation) fn take_upstream_failure(
        &mut self,
    ) -> C::UpstreamError {
        let Some(error) = self.upstream_failure.take() else {
            unreachable!("an upstream template-evaluation marker must retain its exact cause");
        };

        error
    }
}
