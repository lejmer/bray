use bray_binder::BindingQueryContext;
use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_symbols::{ConstantBinaryOperation, SymbolOrdinal, TypeId};

use super::super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    /// Expands the fixed half-open range operation before native or constant realization.
    /// Advancing is guarded by start < end, so the established integer representation cannot wrap.
    pub(in crate::compilation::template) fn range_step(
        &mut self,
        cursor: CheckedTemplateNodeId,
        cursor_type: TypeId,
        result_type: TypeId,
    ) -> Result<(CheckedTemplateNodeId, CheckedTemplateNodeId), FactQueryError> {
        let values = self.context.semantic_values();

        let element = self
            .context
            .compilation()
            .available_compiler_known_symbols()
            .unary_representation_argument(
                values,
                bray_compiler_known::RepresentationRole::Range,
                cursor_type,
            )
            .expect("selected range step retains its integer element type");

        let boolean = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let start = self.push_node(
            CheckedTemplateOperation::TupleElement {
                subject: cursor,
                index: SymbolOrdinal::new(0),
            },
            element,
        );

        let end = self.push_node(
            CheckedTemplateOperation::TupleElement {
                subject: cursor,
                index: SymbolOrdinal::new(1),
            },
            element,
        );

        let condition = self.push_node(
            CheckedTemplateOperation::Binary {
                operation: ConstantBinaryOperation::Less,
                left: start,
                right: end,
            },
            boolean,
        );

        let one = self.integer_node(element, 1)?;

        let next = self.push_node(
            CheckedTemplateOperation::Binary {
                operation: ConstantBinaryOperation::Add,
                left: start,
                right: one,
            },
            element,
        );

        let advanced = self.push_node(CheckedTemplateOperation::tuple([next, end]), cursor_type);

        let present = self.push_node(
            CheckedTemplateOperation::Convert {
                value: start,
                target: result_type,
            },
            result_type,
        );

        let absent = self.nullable_absence_node(result_type)?;

        let result = self.push_node(
            CheckedTemplateOperation::Conditional {
                condition,
                when_true: present,
                when_false: absent,
                state_outputs: [(Some(advanced), Some(cursor))].into(),
            },
            result_type,
        );

        let state = self.push_node(
            CheckedTemplateOperation::StateAfter {
                operation: result,
                ordinal: 0,
            },
            cursor_type,
        );

        Ok((result, state))
    }
}
