use bray_bound_tree::BoundExpressionId;
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBlockId, MirNullableQuery, MirNullableQueryKind, MirOperationKind, MirSourceAnchor,
};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

pub(super) const fn nullable_query_kind(hook: ImplementationHook) -> Option<MirNullableQueryKind> {
    match hook {
        ImplementationHook::NullableIsPresent => Some(MirNullableQueryKind::IsPresent),
        ImplementationHook::NullableIsAbsent => Some(MirNullableQueryKind::IsAbsent),
        _ => None,
    }
}

impl Lowerer<'_> {
    pub(super) fn lower_nullable_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        kind: MirNullableQueryKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let receiver = selection
            .receiver()
            .filter(|_| selection.arguments().is_empty())
            .ok_or(LoweringError::MissingSemanticSelection(id))?;

        let nullable_type = receiver.target_type();

        let (lowered, operand_type) = self.lower_call_receiver(receiver, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(receiver) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(receiver.expression()));
        };

        let result_type = self.expression_type(id)?;

        let result = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::NullableQuery(MirNullableQuery::new(
                kind,
                receiver,
                operand_type,
                nullable_type,
                result_type,
            )),
            result_type,
        )?;

        Ok(LoweredExpression::continuing(current, Some(result), source))
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::ImplementationHook;
    use bray_ir::MirNullableQueryKind;

    use super::nullable_query_kind;

    #[test]
    fn nullable_hooks_map_to_nullable_queries() {
        assert_eq!(
            nullable_query_kind(ImplementationHook::NullableIsPresent),
            Some(MirNullableQueryKind::IsPresent)
        );

        assert_eq!(
            nullable_query_kind(ImplementationHook::NullableIsAbsent),
            Some(MirNullableQueryKind::IsAbsent)
        );

        assert_eq!(nullable_query_kind(ImplementationHook::AddressOf), None);
    }
}
