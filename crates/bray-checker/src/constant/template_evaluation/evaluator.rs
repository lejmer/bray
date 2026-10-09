use std::collections::BTreeSet;

use crate::constant::operator::{binary_operator, unary_operator};
use bray_bound_tree::{
    CheckedTemplate, CheckedTemplateInputKind, CheckedTemplateKind, CheckedTemplateNodeId,
    CheckedTemplateOperation, CheckedTemplateShortCircuitKind,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ConstantBinaryOperation,
    ConstantField, ConstantInstanceKey, ConstantUnaryOperation, ConstantValueData, ConstantValueId,
    ConstantValueKind, GenericArgument, GenericParameterSymbolId, GenericSubstitutionId,
    ImplementationInstanceData, ImplementationSymbolId, TypeId,
};

use super::super::ConstantReferenceResolution;
use super::super::call::{ConstantCallRequest, ConstantCallResolution, ConstantTemplateResolver};
use super::super::conversion::convert_scalar;
use super::super::diagnostic::{ConstantDiagnostic, ConstantLimitKind, diagnostic_operation};
use super::super::limits::{ConstantEvaluationLimits, EvaluationBudget};
use super::super::operation::{fold_binary, fold_unary};
use super::support::{
    TemplateEvaluationFailure, check_definition_materialization, constant_definition,
    integer_index, operation_failure, recovery_value, target_integer_width, template_index,
};
use crate::representation::type_representation_for_context;
use crate::{CheckerQueryError, CheckerRequestContext};

pub(super) struct TemplateEvaluator<'evaluation, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) context: &'evaluation C,
    pub(super) template: &'evaluation CheckedTemplate,
    pub(super) substitution: GenericSubstitutionId,
    pub(super) selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
    pub(super) arguments: &'evaluation [ConstantValueId],
    pub(super) resolver:
        &'evaluation dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    pub(super) limits: ConstantEvaluationLimits,
    pub(super) budget: EvaluationBudget,
    pub(super) values: Vec<Option<ConstantValueId>>,
    pub(super) checked_materialization: BTreeSet<ConstantValueId>,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) upstream_failure: Option<C::UpstreamError>,
    pub(super) static_initializer: bool,
}

impl<C> TemplateEvaluator<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_result(
        &mut self,
        kind: CheckedTemplateKind,
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if self.template.kind() != kind {
            panic!("constant template must match requested kind {kind:?}");
        }

        self.static_initializer = matches!(
            kind,
            CheckedTemplateKind::ProductStaticInitializer
                | CheckedTemplateKind::ThreadLocalStaticInitializer
        );

        let value = self.evaluate_node(self.template.result())?;
        let data = self.context.semantic_values().constant_value_data(value);

        if data.ty() != result_type {
            panic!("constant template result must have type {result_type:?}, actual: {data:?}");
        }

        Ok(value)
    }

    fn evaluate_node(
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

        let node = self.template.nodes().get(index).unwrap_or_else(|| {
            panic!(
                "evaluate_node requires constant template node, node: {node:?}, index: {index:?}"
            )
        });

        let ty = self
            .context
            .semantic_values()
            .substitute_type(node.ty(), self.substitution)
            .unwrap_or_else(|error| panic!("semantic_value in evaluate_node: {error:?}"));

        let value = self.evaluate_operation(node.operation(), ty)?;

        check_definition_materialization(self, value)?;

        let slot = self
            .values
            .get_mut(index).unwrap_or_else(|| panic!("evaluate_node requires constant template value slot, node: {node:?}, index: {index:?}"));

        *slot = Some(value);

        Ok(value)
    }

    fn evaluate_operation(
        &mut self,
        operation: &CheckedTemplateOperation,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        match operation {
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

                let term = self
                    .context
                    .semantic_values()
                    .substitute_constant_term(*term, self.substitution)
                    .unwrap_or_else(|error| {
                        panic!("semantic_value in evaluate_operation: {error:?}")
                    });

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
            } => self.evaluate_binary(*operation, *left, *right, ty),
            CheckedTemplateOperation::Borrow { kind, operand } => {
                self.evaluate_static_address(*kind, *operand, ty)
            }
            CheckedTemplateOperation::Declaration { declaration, .. } => {
                self.evaluate_declaration(declaration, ty)
            }
            CheckedTemplateOperation::Call {
                callable,
                substitution,
                arguments,
                implementation,
            } => self.evaluate_call(
                callable,
                *substitution,
                arguments,
                implementation.as_ref(),
                ty,
            ),
            CheckedTemplateOperation::Convert { value, target } => {
                let value = self.evaluate_node(*value)?;

                let target = self
                    .context
                    .semantic_values()
                    .substitute_type(*target, self.substitution)
                    .unwrap_or_else(|error| {
                        panic!("semantic_value in evaluate_operation: {error:?}")
                    });

                self.evaluate_conversion(value, target)
            }
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
            CheckedTemplateOperation::Product(fields) => {
                let mut values = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let Some(AnySymbolId::StructField(field_id)) =
                        self.resolver.symbol(field.field())
                    else {
                        panic!(
                            "constant product field must resolve to a struct field, actual: {field:?}"
                        );
                    };

                    values.push(ConstantField::new(
                        field_id,
                        self.evaluate_node(*field.value())?,
                    ));
                }

                self.budget
                    .try_charge_elements(values.len())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                Ok(self.intern_value(ty, ConstantValueKind::product(values)))
            }
            CheckedTemplateOperation::TupleElement { subject, index } => {
                let subject = self.evaluate_node(*subject)?;
                let subject = self.context.semantic_values().constant_value_data(subject);

                let ConstantValueKind::Tuple(elements) = subject.kind() else {
                    panic!("constant tuple projection must receive a tuple, actual: {subject:?}");
                };

                Ok(index
                    .to_index()
                    .and_then(|index| elements.get(index))
                    .copied()
                    .unwrap_or_else(|| {
                        panic!("checked constant tuple index {index:?} must exist in the tuple")
                    }))
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
                let subject = self.context.semantic_values().constant_value_data(subject);

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
            } => {
                let condition = self.evaluate_node(*condition)?;

                if self.boolean(condition)? {
                    self.evaluate_node(*when_true)
                } else {
                    self.evaluate_node(*when_false)
                }
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
                    .template
                    .temporaries()
                    .get(index).unwrap_or_else(|| panic!("evaluate_operation requires constant template temporary, ty: {ty:?}, index: {index:?}"));

                self.evaluate_node(temporary.initializer())
            }
        }
    }

    fn evaluate_index_bound(
        &mut self,
        node: CheckedTemplateNodeId,
    ) -> Result<usize, TemplateEvaluationFailure> {
        let value = self.evaluate_node(node)?;
        let value = self.context.semantic_values().constant_value_data(value);

        Ok(integer_index(value.kind()).unwrap_or_else(|| panic!("checked constant index bound at {node:?} must be a nonnegative host-sized integer, actual: {value:?}")))
    }

    fn evaluate_slice(
        &mut self,
        subject: CheckedTemplateNodeId,
        lower: Option<CheckedTemplateNodeId>,
        upper: Option<CheckedTemplateNodeId>,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.evaluate_node(subject)?;
        let subject = self.context.semantic_values().constant_value_data(subject);

        let ConstantValueKind::Array(elements) = subject.kind() else {
            panic!("constant slice must receive an array, actual: {subject:?}");
        };

        let lower = lower
            .map(|bound| self.evaluate_index_bound(bound))
            .transpose()?;

        let upper = upper
            .map(|bound| self.evaluate_index_bound(bound))
            .transpose()?;

        let elements =
            crate::constant::array::slice_elements(elements, lower, upper).ok_or_else(|| {
                TemplateEvaluationFailure::invalid_expression(
                    bray_diagnostics::DiagnosticExpressionCategory::Indexing,
                )
            })?;

        self.budget
            .try_charge_elements(elements.len())
            .map_err(TemplateEvaluationFailure::Diagnostic)?;

        Ok(self.intern_value(ty, ConstantValueKind::array(elements.iter().copied())))
    }

    fn evaluate_static_address(
        &mut self,
        kind: bray_symbols::BorrowKind,
        operand: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if !self.static_initializer || kind != bray_symbols::BorrowKind::Shared {
            panic!(
                "constant static address requires a shared borrow in a static initializer, kind: {kind:?}, operand: {operand:?}"
            );
        }

        let index = template_index(operand.raw());

        let Some(CheckedTemplateOperation::Declaration {
            declaration,
            substitution: Some(substitution),
        }) = self
            .template
            .nodes()
            .get(index)
            .map(|node| node.operation())
        else {
            panic!("static address operand {operand:?} must be a static declaration node");
        };

        let Some(AnySymbolId::Static(declaration)) = self.resolver.symbol(declaration) else {
            panic!(
                "static address declaration must resolve to a static symbol, actual: {declaration:?}"
            );
        };

        let substitution = self
            .context
            .semantic_values()
            .substitute_generic_substitution(*substitution, self.substitution)
            .unwrap_or_else(|error| panic!("semantic_value in evaluate_static_address: {error:?}"));

        let selection = self
            .resolver
            .resolve_static(declaration, substitution)
            .map_err(|error| self.record_query_failure(error))?;

        let (selection, diagnostics) = selection.into_parts();

        self.diagnostics = self.diagnostics.merged(&diagnostics);

        Ok(self.intern_value(ty, ConstantValueKind::StaticAddress(selection)))
    }

    fn evaluate_input(
        &mut self,
        input: bray_bound_tree::CheckedTemplateInputId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let index = template_index(input.raw());

        let input = self
            .template
            .inputs()
            .get(index).unwrap_or_else(|| panic!("evaluate_input requires constant template input, input: {input:?}, index: {index:?}"));

        match input.kind() {
            CheckedTemplateInputKind::Parameter(ordinal) => Ok(ordinal
                .to_index()
                .and_then(|index| self.arguments.get(index))
                .copied().unwrap_or_else(|| panic!("checked constant parameter ordinal {ordinal:?} must exist in the application"))),
            CheckedTemplateInputKind::GenericConstant(parameter) => {
                let Some(AnySymbolId::GenericConstParameter(parameter)) =
                    self.resolver.symbol(parameter)
                else {
                    panic!("constant template parameter must resolve to a generic constant, actual: {parameter:?}");
                };

                let substitution = self
                    .context
                    .semantic_values()
                    .generic_substitution_data(self.substitution);

                let Some(GenericArgument::Constant(term)) =
                    substitution.argument_for(GenericParameterSymbolId::Const(parameter))
                else {
                    panic!("generic constant {parameter:?} must have a constant substitution argument");
                };

                let ty = self
                    .context
                    .semantic_values()
                    .substitute_type(input.ty(), self.substitution).unwrap_or_else(|error| panic!("semantic_value in evaluate_input: {error:?}"));

                self.evaluate_term(term, ty)
            }
            CheckedTemplateInputKind::Receiver
            | CheckedTemplateInputKind::GenericType(_)
            | CheckedTemplateInputKind::PostconditionResult => {
                panic!("constant template input kind must be evaluable at compile time, actual: {input:?}")
            }
        }
    }

    pub(super) fn evaluate_unary(
        &self,
        operation: ConstantUnaryOperation,
        operand: ConstantValueId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if matches!(
            operation,
            ConstantUnaryOperation::PredicateTrust
                | ConstantUnaryOperation::BorrowObservation
                | ConstantUnaryOperation::EntryCondition
        ) {
            return Ok(operand);
        }

        let operand = self.context.semantic_values().constant_value_data(operand);

        let representation = type_representation_for_context(self.context, ty);

        let kind = fold_unary(
            unary_operator(operation),
            operand.kind(),
            representation,
            target_integer_width(self.context, representation),
        )
        .map_err(|error| {
            operation_failure(diagnostic_operation(unary_operator(operation)), error)
        })?;

        Ok(self.intern_value(ty, kind))
    }

    fn evaluate_binary(
        &mut self,
        operation: ConstantBinaryOperation,
        left: CheckedTemplateNodeId,
        right: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let left = self.evaluate_node(left)?;
        let left_data = self.context.semantic_values().constant_value_data(left);

        match (operation, left_data.kind()) {
            (ConstantBinaryOperation::LogicalAnd, ConstantValueKind::Boolean(false))
            | (ConstantBinaryOperation::LogicalOr, ConstantValueKind::Boolean(true)) => {
                return Ok(left);
            }
            _ => {}
        }

        let right = self.evaluate_node(right)?;
        let right_data = self.context.semantic_values().constant_value_data(right);

        let kind = fold_binary(
            binary_operator(operation),
            left_data.kind(),
            right_data.kind(),
            self.limits.integer_bits(),
        )
        .map_err(|error| {
            operation_failure(diagnostic_operation(binary_operator(operation)), error)
        })?;

        Ok(self.intern_value(ty, kind))
    }

    fn evaluate_declaration(
        &mut self,
        declaration: &bray_symbols::SymbolKey,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let Some(symbol) = self.resolver.symbol(declaration) else {
            panic!("constant declaration {declaration:?} must resolve to a symbol");
        };

        if let AnySymbolId::UnionVariant(variant) = symbol {
            return Ok(self.intern_value(ty, ConstantValueKind::union(variant, [])));
        }

        let Some(definition) = constant_definition(symbol) else {
            panic!("constant symbol {symbol:?} must have a constant definition");
        };

        let instance =
            ConstantInstanceKey::new(definition, self.substitution, self.selected_implementation);

        let result = self
            .resolver
            .resolve_constant(instance, self.budget.remaining_limits(self.limits))
            .map_err(|error| self.record_query_failure(error))?;

        self.diagnostics = self.diagnostics.merged(result.diagnostics());

        let value = match result.value() {
            ConstantReferenceResolution::Value(value) => *value,
            ConstantReferenceResolution::Evaluated(result) => {
                self.budget
                    .try_charge_usage(result.usage())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                result.value()
            }
            ConstantReferenceResolution::Term(term) => self.evaluate_term(*term, ty)?,
            ConstantReferenceResolution::Cycle { definition } => {
                return Err(TemplateEvaluationFailure::Diagnostic(
                    ConstantDiagnostic::Cycle {
                        definition: *definition,
                    },
                ));
            }
            ConstantReferenceResolution::Invalid => {
                panic!(
                    "constant declaration {declaration:?} must have a valid reference resolution"
                );
            }
        };

        let data = self.context.semantic_values().constant_value_data(value);

        if data.ty() != ty {
            panic!(
                "constant declaration {declaration:?} must produce type {ty:?}, actual: {data:?}"
            );
        }

        Ok(value)
    }

    fn evaluate_call(
        &mut self,
        callable: &bray_symbols::SymbolKey,
        substitution: GenericSubstitutionId,
        arguments: &[CheckedTemplateNodeId],
        implementation: Option<&(bray_symbols::SymbolKey, GenericSubstitutionId)>,
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let arguments = self.evaluate_nodes(arguments)?;

        let Some(callable) = self
            .resolver
            .symbol(callable)
            .and_then(CallableDefinitionId::try_new)
        else {
            panic!("constant call must resolve to a callable definition: {callable:?}");
        };

        let values = self.context.semantic_values();

        let substitution = values
            .substitute_generic_substitution(substitution, self.substitution)
            .unwrap_or_else(|error| panic!("semantic_value in evaluate_call: {error:?}"));

        let implementation = implementation
            .map(|(declaration, substitution)| {
                let Some(declaration) = self
                    .resolver
                    .symbol(declaration)
                    .and_then(ImplementationSymbolId::try_from_any)
                else {
                    panic!("constant call implementation must resolve to a declared implementation: {declaration:?}");
                };

                let substitution = values
                    .substitute_generic_substitution(*substitution, self.substitution).unwrap_or_else(|error| panic!("semantic_value in evaluate_call: {error:?}"));

                values
                    .intern_implementation_instance(ImplementationInstanceData::new(
                        declaration,
                        substitution,
                    ))
                    .map(Some).unwrap_or_else(|error| panic!("evaluate_call must satisfy its checked construction contract: {error:?}"))
            })
            .flatten();

        let limits = self.budget.remaining_limits(self.limits);

        let Some(limits) = limits.nested_call() else {
            return Err(TemplateEvaluationFailure::Diagnostic(
                ConstantDiagnostic::limit(ConstantLimitKind::EvaluationSteps, 1, 0),
            ));
        };

        let request = ConstantCallRequest::new(
            CallableInstanceData::new(callable, substitution),
            implementation,
            arguments,
            result_type,
            limits,
        );

        self.resolve_call(&request, result_type)
    }

    pub(super) fn resolve_call(
        &mut self,
        request: &ConstantCallRequest,
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let resolution = self
            .resolver
            .resolve(request)
            .map_err(|error| self.record_query_failure(error))?;

        match resolution {
            ConstantCallResolution::Evaluated(result) => {
                self.budget
                    .try_charge_usage(result.value().usage())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                self.diagnostics = self.diagnostics.merged(result.diagnostics());

                Ok(result.value().value())
            }
            ConstantCallResolution::Cycle => Err(TemplateEvaluationFailure::Diagnostic(
                ConstantDiagnostic::Cycle { definition: None },
            )),
            ConstantCallResolution::Ineligible(diagnostics) => {
                if diagnostics.has_errors() {
                    self.diagnostics = self.diagnostics.merged(&diagnostics);

                    return Ok(recovery_value(self.context.semantic_values(), result_type));
                }

                panic!(
                    "checked constant call must have a valid reference resolution, request: {request:?}"
                )
            }
        }
    }

    pub(super) fn evaluate_conversion(
        &self,
        value: ConstantValueId,
        target: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let data = self.context.semantic_values().constant_value_data(value);

        if data.ty() == target {
            return Ok(value);
        }

        let Some(representation) = type_representation_for_context(self.context, target) else {
            panic!(
                "checked constant conversion must have a scalar target representation, target: {target:?}"
            );
        };

        let kind = convert_scalar(data.kind(), representation, || {
            self.context
                .selected_target()
                .machine()
                .pointer_width_bits()
        })
        .map_err(|error| {
            operation_failure(
                bray_diagnostics::DiagnosticConstantOperation::Conversion,
                error,
            )
        })?;

        Ok(self.intern_value(target, kind))
    }

    fn evaluate_projection(
        &self,
        subject: ConstantValueId,
        member: &bray_symbols::SymbolKey,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.context.semantic_values().constant_value_data(subject);

        let member = self
            .resolver
            .symbol(member).unwrap_or_else(|| panic!("evaluate_projection requires constant projection member, subject: {subject:?}, ty: {ty:?}, member: {member:?}"));

        let value = match (subject.kind(), member) {
            (ConstantValueKind::Product(fields), AnySymbolId::StructField(field)) => fields
                .iter()
                .find(|entry| *entry.field() == field)
                .map(|entry| *entry.value()),
            (ConstantValueKind::Union { fields, .. }, AnySymbolId::UnionPayloadField(field)) => {
                fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value())
            }
            _ => None,
        }.unwrap_or_else(|| panic!("evaluate_projection requires constant projection field, subject: {subject:?}, ty: {ty:?}"));

        let value_data = self.context.semantic_values().constant_value_data(value);

        if value_data.ty() != ty {
            panic!("checked constant projection must produce type {ty:?}, actual: {value_data:?}");
        }

        Ok(value)
    }

    fn evaluate_nodes(
        &mut self,
        nodes: &[CheckedTemplateNodeId],
    ) -> Result<Vec<ConstantValueId>, TemplateEvaluationFailure> {
        nodes.iter().map(|node| self.evaluate_node(*node)).collect()
    }

    fn boolean(&self, value: ConstantValueId) -> Result<bool, TemplateEvaluationFailure> {
        let value = self.context.semantic_values().constant_value_data(value);

        let ConstantValueKind::Boolean(value) = value.kind() else {
            panic!("checked Boolean constant must be a Boolean value, actual: {value:?}");
        };

        Ok(*value)
    }

    pub(super) fn intern_value(&self, ty: TypeId, kind: ConstantValueKind) -> ConstantValueId {
        self.context
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))
            .unwrap_or_else(|error| {
                panic!("intern_value must satisfy its checked construction contract: {error:?}")
            })
    }

    pub(super) fn observe_cancellation(&self) -> Result<(), TemplateEvaluationFailure> {
        if self.context.cancellation().is_cancelled() {
            Err(TemplateEvaluationFailure::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn record_query_failure(
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

    pub(super) fn take_upstream_failure(&mut self) -> C::UpstreamError {
        let Some(error) = self.upstream_failure.take() else {
            unreachable!("an upstream template-evaluation marker must retain its exact cause");
        };

        error
    }
}
