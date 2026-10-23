use super::super::super::conversion::convert_scalar;
use super::super::super::diagnostic::diagnostic_operation;
use super::super::super::operation::{fold_binary, fold_unary};
use super::super::support::{
    TemplateEvaluationFailure, integer_index, operation_failure, target_integer_width,
    template_index,
};
use crate::CheckerRequestContext;
use crate::constant::operator::{binary_operator, unary_operator};
use crate::representation::type_representation_for_context;
use bray_bound_tree::{CheckedTemplateInputKind, CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_symbols::{
    AnySymbolId, ConstantBinaryOperation, ConstantField, ConstantUnaryOperation, ConstantValueId,
    ConstantValueKind, GenericArgument, GenericParameterSymbolId, TypeId,
};

use super::control::TemplateEvaluator;

impl<C> TemplateEvaluator<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_borrow(
        &mut self,
        kind: bray_symbols::BorrowKind,
        operand: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if self.static_initializer
            && matches!(
                self.template().nodes()[template_index(operand.raw())].operation(),
                CheckedTemplateOperation::Declaration { .. }
            )
        {
            self.evaluate_static_address(kind, operand, ty)
        } else {
            assert_eq!(
                kind,
                bray_symbols::BorrowKind::Shared,
                "const observation has shared capability"
            );

            let value = self.evaluate_node(operand)?;

            self.evaluate_conversion(value, ty)
        }
    }

    pub(in crate::constant::template_evaluation) fn evaluate_index_bound(
        &mut self,
        node: CheckedTemplateNodeId,
    ) -> Result<usize, TemplateEvaluationFailure> {
        let value = self.evaluate_node(node)?;
        let value = self.context.semantic_values().constant_value_data(value);

        Ok(integer_index(value.kind()).unwrap_or_else(|| panic!("checked constant index bound at {node:?} must be a nonnegative host-sized integer, actual: {value:?}")))
    }

    pub(super) fn evaluate_slice(
        &mut self,
        subject: CheckedTemplateNodeId,
        lower: Option<CheckedTemplateNodeId>,
        upper: Option<CheckedTemplateNodeId>,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.evaluate_node(subject)?;
        let subject = self.observed_value(subject)?;

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

    pub(super) fn evaluate_static_address(
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
            .template()
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

        let substitution = self.substitute_arguments(*substitution);

        let selection = self
            .resolver
            .resolve_static(declaration, substitution)
            .map_err(|error| self.record_query_failure(error))?;

        let (selection, diagnostics) = selection.into_parts();

        self.diagnostics = self.diagnostics.merged(&diagnostics);

        Ok(self.intern_value(ty, ConstantValueKind::StaticAddress(selection)))
    }

    pub(super) fn evaluate_fields<F>(
        &mut self,
        fields: &[ConstantField<bray_symbols::SymbolKey, CheckedTemplateNodeId>],
        field_id: impl Fn(AnySymbolId) -> Option<F>,
    ) -> Result<Vec<ConstantField<F, ConstantValueId>>, TemplateEvaluationFailure> {
        let mut values = Vec::with_capacity(fields.len());

        for field in fields {
            let id = self
                .resolver
                .symbol(field.field())
                .and_then(&field_id)
                .expect("checked constructor retains its field category");

            values.push(ConstantField::new(id, self.evaluate_node(*field.value())?));
        }

        self.budget
            .try_charge_elements(values.len())
            .map_err(TemplateEvaluationFailure::Diagnostic)?;

        Ok(values)
    }

    pub(super) fn evaluate_input(
        &mut self,
        input: bray_bound_tree::CheckedTemplateInputId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let index = template_index(input.raw());

        let input = self
            .template()
            .inputs()
            .get(index).unwrap_or_else(|| panic!("evaluate_input requires constant template input, input: {input:?}, index: {index:?}"));

        match input.kind() {
            CheckedTemplateInputKind::Parameter(ordinal) => Ok(ordinal
                .to_index()
                .and_then(|index| self.arguments.get(index + usize::from(self.template().inputs().iter()
                    .any(|input| input.kind() == &CheckedTemplateInputKind::Receiver))))
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
                    .generic_substitution_data(self.substitution.expect("generic input nodes retain their environment"));

                let Some(GenericArgument::Constant(term)) =
                    substitution.argument_for(GenericParameterSymbolId::Const(parameter))
                else {
                    panic!("generic constant {parameter:?} must have a constant substitution argument");
                };

                let ty = self.substitute_type(input.ty());

                self.evaluate_term(term, ty)
            }
            CheckedTemplateInputKind::Receiver => Ok(*self.arguments.first()
                .expect("checked constant method application supplies its receiver")),
            CheckedTemplateInputKind::GenericType(_)
            | CheckedTemplateInputKind::PostconditionResult => {
                panic!("constant template input kind must be evaluable at compile time, actual: {input:?}")
            }
        }
    }

    pub(in crate::constant::template_evaluation) fn evaluate_unary(
        &self,
        operation: ConstantUnaryOperation,
        operand: ConstantValueId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        if operation == ConstantUnaryOperation::BorrowObservation {
            return Ok(self
                .context
                .semantic_values()
                .observed_constant_value(operand));
        }

        if matches!(
            operation,
            ConstantUnaryOperation::PredicateTrust
                | ConstantUnaryOperation::BorrowObservation
                | ConstantUnaryOperation::EntryCondition
        ) {
            return Ok(operand);
        }

        let operand = self.context.semantic_values().constant_value_data(
            self.context
                .semantic_values()
                .observed_constant_value(operand),
        );

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

    pub(super) fn evaluate_machine_unary(
        &self,
        operation: ConstantUnaryOperation,
        operand: ConstantValueId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let data = self.context.semantic_values().constant_value_data(
            self.context
                .semantic_values()
                .observed_constant_value(operand),
        );

        let role = type_representation_for_context(self.context, ty);

        if matches!(
            operation,
            ConstantUnaryOperation::Negate | ConstantUnaryOperation::BitwiseNot
        ) && let ConstantValueKind::Integer(value) = data.kind()
            && let Some(role) = role.filter(|role| role.integer_representation().is_some())
        {
            let kind = super::super::super::machine::fold_machine_integer_unary(
                unary_operator(operation),
                value,
                role,
                self.context
                    .selected_target()
                    .machine()
                    .pointer_width_bits(),
            );

            return Ok(self.intern_value(ty, kind));
        }

        self.evaluate_unary(operation, operand, ty)
    }

    pub(super) fn evaluate_binary(
        &mut self,
        operation: ConstantBinaryOperation,
        left: CheckedTemplateNodeId,
        right: CheckedTemplateNodeId,
        ty: TypeId,
        machine: bool,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let left = self.evaluate_node(left)?;

        let left_data = self
            .context
            .semantic_values()
            .constant_value_data(self.context.semantic_values().observed_constant_value(left));

        match (operation, left_data.kind()) {
            (ConstantBinaryOperation::LogicalAnd, ConstantValueKind::Boolean(false))
            | (ConstantBinaryOperation::LogicalOr, ConstantValueKind::Boolean(true)) => {
                return Ok(left);
            }
            _ => {}
        }

        let right = self.evaluate_node(right)?;

        let right_data = self.context.semantic_values().constant_value_data(
            self.context
                .semantic_values()
                .observed_constant_value(right),
        );

        if machine
            && let (ConstantValueKind::Integer(left), ConstantValueKind::Integer(right)) =
                (left_data.kind(), right_data.kind())
            && let Some(representation) =
                type_representation_for_context(self.context, left_data.ty())
                    .and_then(bray_compiler_known::RepresentationRole::integer_representation)
        {
            if let Some(kind) = super::super::super::machine::fold_machine_integer_binary(
                binary_operator(operation),
                left,
                right,
                representation,
                self.context
                    .selected_target()
                    .machine()
                    .pointer_width_bits(),
            ) {
                return Ok(self.intern_value(ty, kind));
            }

            // Recover a specific arithmetic failure only on the failed path.
            let cause = fold_binary(
                binary_operator(operation),
                left_data.kind(),
                right_data.kind(),
                self.limits.integer_bits(),
            )
            .err()
            .unwrap_or(super::super::super::operation::ConstantOperationError::Invalid);

            return Err(operation_failure(
                diagnostic_operation(binary_operator(operation)),
                cause,
            ));
        }

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

    pub(in crate::constant::template_evaluation) fn evaluate_conversion(
        &mut self,
        value: ConstantValueId,
        target: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let data = self.context.semantic_values().constant_value_data(value);

        if data.ty() == target {
            return Ok(value);
        }

        if let bray_symbols::TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Shared,
            target: referent,
        } = self.context.semantic_values().type_data(target).as_ref()
        {
            let observed = if data.ty() == *referent {
                value
            } else {
                self.context
                    .semantic_values()
                    .observed_constant_value(value)
            };

            return Ok(self.intern_value(target, ConstantValueKind::Borrowed(observed)));
        }

        if let ConstantValueKind::Borrowed(_) = data.kind() {
            return self.evaluate_conversion(
                self.context
                    .semantic_values()
                    .observed_constant_value(value),
                target,
            );
        }

        if let bray_symbols::TypeData::ExactInteger { .. } =
            self.context.semantic_values().type_data(target).as_ref()
        {
            let ConstantValueKind::Integer(integer) = data.kind() else {
                panic!("checked exact-integer conversion has an integer operand");
            };

            return Ok(self.intern_value(target, ConstantValueKind::Integer(integer.clone())));
        }

        if matches!(self.context.semantic_values().type_data(target).as_ref(),
            bray_symbols::TypeData::Nullable(contained) if *contained == data.ty())
        {
            return Ok(self.intern_value(target, ConstantValueKind::NullablePresent(value)));
        }

        let target_data = self.context.semantic_values().type_data(target);

        let composite = match (data.kind(), target_data.as_ref()) {
            (ConstantValueKind::Tuple(elements), bray_symbols::TypeData::Tuple(types)) => {
                assert_eq!(
                    elements.len(),
                    types.len(),
                    "checked tuple conversion preserves arity"
                );

                Some(ConstantValueKind::tuple(
                    elements
                        .iter()
                        .zip(types.iter())
                        .map(|(value, target)| self.evaluate_conversion(*value, *target))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }
            (ConstantValueKind::Array(elements), bray_symbols::TypeData::Array { element, .. }) => {
                Some(ConstantValueKind::array(
                    elements
                        .iter()
                        .map(|value| self.evaluate_conversion(*value, *element))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }
            (ConstantValueKind::NullableAbsent, bray_symbols::TypeData::Nullable(_)) => {
                Some(ConstantValueKind::NullableAbsent)
            }
            (
                ConstantValueKind::NullablePresent(value),
                bray_symbols::TypeData::Nullable(contained),
            ) => Some(ConstantValueKind::NullablePresent(
                self.evaluate_conversion(*value, *contained)?,
            )),
            _ => None,
        };

        if let Some(kind) = composite {
            return Ok(self.intern_value(target, kind));
        }

        let Some(representation) = type_representation_for_context(self.context, target) else {
            panic!(
                "checked constant conversion must have a scalar target representation, target: {target:?}"
            );
        };

        if representation.complex_component().is_some()
            && let ConstantValueKind::Tuple(elements) = data.kind()
        {
            let [real, imaginary] = elements.as_ref() else {
                panic!("checked tuple-to-complex conversion has two components");
            };

            let kind = super::super::super::conversion::complex_from_components(
                self.context.semantic_values(),
                [*real, *imaginary],
            )
            .expect("checked tuple-to-complex conversion has converted real components");

            return Ok(self.intern_value(target, kind));
        }

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

    pub(in crate::constant::template_evaluation) fn evaluate_projection(
        &self,
        subject: ConstantValueId,
        member: &bray_symbols::SymbolKey,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.observed_value(subject)?;

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

    pub(in crate::constant::template_evaluation) fn observed_value(
        &self,
        value: ConstantValueId,
    ) -> Result<std::sync::Arc<bray_symbols::ConstantValueData>, TemplateEvaluationFailure> {
        let values = self.context.semantic_values();
        let data = values.constant_value_data(values.observed_constant_value(value));

        if matches!(data.kind(), ConstantValueKind::StaticAddress(_)) {
            return Err(TemplateEvaluationFailure::invalid_expression(
                bray_diagnostics::DiagnosticExpressionCategory::MemberAccess,
            ));
        }

        Ok(data)
    }

    pub(super) fn evaluate_construction(
        &mut self,
        variant: Option<&bray_symbols::SymbolKey>,
        fields: &[ConstantField<bray_symbols::SymbolKey, CheckedTemplateNodeId>],
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let kind = match variant {
            Some(key) => {
                let Some(AnySymbolId::UnionVariant(variant)) = self.resolver.symbol(key) else {
                    panic!("checked union constructor retains its active variant");
                };

                let fields = self.evaluate_fields(fields, |symbol| match symbol {
                    AnySymbolId::UnionPayloadField(field) => Some(field),
                    _ => None,
                })?;

                ConstantValueKind::union(variant, fields)
            }
            None => {
                ConstantValueKind::product(self.evaluate_fields(fields, |symbol| match symbol {
                    AnySymbolId::StructField(field) => Some(field),
                    _ => None,
                })?)
            }
        };

        Ok(self.intern_value(ty, kind))
    }

    pub(super) fn evaluate_variant_test(
        &mut self,
        subject: CheckedTemplateNodeId,
        variant: &bray_symbols::SymbolKey,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.evaluate_node(subject)?;
        let subject = self.observed_value(subject)?;

        let variant = self
            .resolver
            .symbol(variant)
            .expect("checked variant declaration resolves");

        let AnySymbolId::UnionVariant(variant) = variant else {
            panic!("checked variant test retains its union variant");
        };

        let ConstantValueKind::Union {
            variant: active, ..
        } = subject.kind()
        else {
            panic!("checked variant test observes a union value");
        };

        Ok(self.intern_value(ty, ConstantValueKind::Boolean(*active == variant)))
    }

    pub(super) fn evaluate_tuple_element(
        &mut self,
        subject: CheckedTemplateNodeId,
        index: bray_symbols::SymbolOrdinal,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.evaluate_node(subject)?;
        let subject = self.observed_value(subject)?;

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
}
