use bray_bound_tree::MemoryLayoutQueryKind;
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticId, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableInstanceData, ConstantField, ConstantValueData, ConstantValueId,
    ConstantValueKind, GenericArgument, IntegerConstant, MemberLookupResult, NamedTypeSymbolId,
    SemanticValueStoreError, TypeData, TypeId,
};
use bray_target::TargetValueLayout;

use super::limits::EvaluationBudget;
use super::{ConstantEvaluationLimits, ConstantEvaluationUsage, EvaluatedConstantCall};
use crate::{CheckerOutcome, CheckerQueryError, CheckerQueryResult, CheckerRequestContext};

/// The selected physical type layout and optional trailing field offset and stride.
pub type ConstantMemoryLayout = (TargetValueLayout, Option<(u64, u64)>);

/// Evaluates layout intrinsics using the coordinator's existing physical layout authority.
pub fn evaluate_constant_layout_intrinsic<C>(
    context: &C,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
    result_type: TypeId,
    limits: ConstantEvaluationLimits,
    layout: impl FnOnce(
        TypeId,
    ) -> CheckerQueryResult<
        DiagnosticResult<Option<ConstantMemoryLayout>>,
        C::UpstreamError,
    >,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
    C::UpstreamError: From<SemanticValueStoreError>,
{
    let evaluated = evaluate_layout(context, callable, hook, arguments, result_type, layout);

    let evaluated = match evaluated {
        Ok(evaluated) => evaluated,
        Err(error) => return CheckerOutcome::from_query_error(error),
    };

    let mut diagnostics = evaluated.diagnostics().clone();

    let Some((value, elements)) = *evaluated.value() else {
        return CheckerOutcome::complete(None, diagnostics);
    };

    let mut budget = EvaluationBudget::from_limits(limits);
    let usage = ConstantEvaluationUsage::new(0, elements, 0);

    if let Err(problem) = budget.try_charge_usage(usage) {
        match problem.render(context, DiagnosticId::new(0), None, || Ok(result_type)) {
            Ok(diagnostic) => diagnostics.add(diagnostic),
            Err(error) => return CheckerOutcome::from_query_error(error),
        }

        return CheckerOutcome::complete(None, diagnostics);
    }

    CheckerOutcome::complete(
        Some(EvaluatedConstantCall::new(value, budget.usage(limits))),
        diagnostics,
    )
}

fn evaluate_layout<C>(
    context: &C,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
    result_type: TypeId,
    resolve: impl FnOnce(
        TypeId,
    ) -> CheckerQueryResult<
        DiagnosticResult<Option<ConstantMemoryLayout>>,
        C::UpstreamError,
    >,
) -> CheckerQueryResult<DiagnosticResult<Option<(ConstantValueId, u64)>>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
    C::UpstreamError: From<SemanticValueStoreError>,
{
    let values = context.semantic_values();

    let kind = MemoryLayoutQueryKind::for_implementation_hook(hook)
        .expect("constant layout dispatch requires a layout hook");

    if !values.substitution_is_concrete(callable.substitution()) {
        return Ok(DiagnosticResult::without_diagnostics(None));
    }

    let substitution = values.generic_substitution_data(callable.substitution());

    let [binding] = substitution.bindings() else {
        panic!("recognized layout intrinsic has one type argument");
    };

    let GenericArgument::Type(ty) = binding.argument() else {
        panic!("recognized layout intrinsic requires a type argument");
    };

    let mut diagnostics = DiagnosticBag::new();

    if !crate::memory::layout_query_type_supported(context, ty, kind)? {
        diagnostics.add(crate::memory::layout_query_type_diagnostic(
            kind, hook, 0, None,
        ));

        return Ok(DiagnosticResult::new(None, diagnostics));
    }

    let physical = resolve(ty)?;

    diagnostics.extend(physical.diagnostics().iter().cloned());

    let Some((layout, trailing)) = *physical.value() else {
        if !diagnostics.has_errors() {
            diagnostics.add(crate::memory::layout_query_type_diagnostic(
                kind, hook, 0, None,
            ));
        }

        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let scalar = match kind {
        MemoryLayoutQueryKind::Size | MemoryLayoutQueryKind::Stride => Some(layout.size()),
        MemoryLayoutQueryKind::Alignment => Some(layout.alignment().get()),
        MemoryLayoutQueryKind::Layout | MemoryLayoutQueryKind::Trailing => None,
        MemoryLayoutQueryKind::LayoutStatus => {
            unreachable!("layout status is a synthesized MIR operation")
        }
    };

    if let Some(scalar) = scalar {
        assert!(
            arguments.is_empty(),
            "scalar layout query has no value arguments"
        );

        let value = integer_value(context, result_type, scalar)?;

        return Ok(DiagnosticResult::new(Some((value, 0)), diagnostics));
    }

    let [count] = arguments else {
        panic!("allocation layout query has one count argument");
    };

    let count = values.constant_value_data(values.observed_constant_value(*count));

    let ConstantValueKind::Integer(integer) = count.kind() else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    let count_value = integer
        .to_u64()
        .expect("checked native usize count fits u64");

    let width = context
        .selected_target()
        .machine()
        .pointer_width_bits()
        .get();

    let maximum = u64::MAX >> (64 - width);

    let (prefix, stride) = if kind == MemoryLayoutQueryKind::Trailing {
        trailing.expect("checked flexible layout query has trailing storage")
    } else {
        (0, layout.size())
    };

    let limit = bray_target::allocation_count_limit(prefix, stride, maximum);

    let result = if layout.alignment()
        > context
            .selected_target()
            .properties()
            .alignments()
            .max_allocation()
    {
        Err(1)
    } else if !limit.is_some_and(|limit| count_value <= limit) {
        Err(0)
    } else {
        Ok(prefix + count_value * stride)
    };

    let elements = if result.is_ok() { 3 } else { 1 };

    let value = allocation_value(
        context,
        result_type,
        count.ty(),
        result,
        layout.alignment().get(),
    )?;

    Ok(DiagnosticResult::new(Some((value, elements)), diagnostics))
}

fn integer_value<C: CheckerRequestContext + ?Sized>(
    context: &C,
    ty: TypeId,
    value: u64,
) -> CheckerQueryResult<ConstantValueId, C::UpstreamError>
where
    C::UpstreamError: From<SemanticValueStoreError>,
{
    context
        .semantic_values()
        .intern_constant_value(ConstantValueData::new(
            ty,
            ConstantValueKind::Integer(IntegerConstant::from_u64(value)),
        ))
        .map_err(|error| CheckerQueryError::Upstream(error.into()))
}

fn allocation_value<C: CheckerRequestContext + ?Sized>(
    context: &C,
    result_type: TypeId,
    integer_type: TypeId,
    result: Result<u64, usize>,
    alignment: u64,
) -> CheckerQueryResult<ConstantValueId, C::UpstreamError>
where
    C::UpstreamError: From<SemanticValueStoreError>,
{
    let values = context.semantic_values();
    let data = values.type_data(result_type);

    let TypeData::Named { substitution, .. } = *data else {
        panic!("allocation layout returns the recognized Result type");
    };

    let substitution = values.generic_substitution_data(substitution);

    let arguments = substitution
        .bindings()
        .iter()
        .map(|binding| match binding.argument() {
            GenericArgument::Type(ty) => ty,
            GenericArgument::Constant(_) => unreachable!("Result has type arguments"),
        })
        .collect::<Vec<_>>();

    let [layout_type, error_type] = arguments.as_slice() else {
        panic!("Result has success and error types");
    };

    let representation = context
        .available_compiler_known_symbols()
        .result_representation()
        .expect("checked allocation layout retains Result representation");

    let (variant, field, payload) = match result {
        Ok(bytes) => {
            let data = values.type_data(*layout_type);

            let TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                ..
            } = *data
            else {
                panic!("recognized MemoryLayout is a struct");
            };

            let mut fields = Vec::with_capacity(2);

            for (name, value) in [("bytes", bytes), ("align", alignment)] {
                let MemberLookupResult::Found(AnySymbolId::StructField(field)) =
                    context.lookup_member(definition.into(), name)?
                else {
                    panic!("recognized MemoryLayout requires its {name} field");
                };

                fields.push(ConstantField::new(
                    field,
                    integer_value(context, integer_type, value)?,
                ));
            }

            let payload = values
                .intern_constant_value(ConstantValueData::new(
                    *layout_type,
                    ConstantValueKind::product(fields),
                ))
                .map_err(|error| CheckerQueryError::Upstream(error.into()))?;

            (
                representation.success_variant(),
                representation.success_field(),
                payload,
            )
        }
        Err(ordinal) => {
            let data = values.type_data(*error_type);

            let TypeData::Named {
                definition: NamedTypeSymbolId::Union(definition),
                ..
            } = *data
            else {
                panic!("recognized MemoryLayoutError is a union");
            };

            let variant = context
                .union(definition)?
                .expect("recognized MemoryLayoutError declaration")
                .variants()[ordinal];

            let payload = values
                .intern_constant_value(ConstantValueData::new(
                    *error_type,
                    ConstantValueKind::union(variant, []),
                ))
                .map_err(|error| CheckerQueryError::Upstream(error.into()))?;

            (
                representation.error_variant(),
                representation.error_field(),
                payload,
            )
        }
    };

    values
        .intern_constant_value(ConstantValueData::new(
            result_type,
            ConstantValueKind::union(variant, [ConstantField::new(field, payload)]),
        ))
        .map_err(|error| CheckerQueryError::Upstream(error.into()))
}
