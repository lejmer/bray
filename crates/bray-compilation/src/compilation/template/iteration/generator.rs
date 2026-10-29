use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedTemplateNodeId,
    CheckedTemplateOperation, StorageIdentity, StorageIdentityId,
};
use bray_symbols::{ConstantBinaryOperation, SymbolOrdinal, TypeData, TypeId};

use super::super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(in crate::compilation::template) fn generator_summary(
        &mut self,
        producer: BoundExpressionId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if let Some(root) = self.generator_values.get(&producer).copied() {
            return Ok(self
                .current_state(root)
                .expect("eager generator observation is initialized"));
        }

        let expression = self
            .unit
            .view()
            .expression(producer)
            .expect("checked generator producer exists");

        let BoundExpression::Structured(region) = expression else {
            panic!("generator observation retains its region");
        };

        assert_eq!(
            region.kind(),
            BoundStructuredExpressionKind::GeneralGenerator
        );

        let [iteration] = region.operands() else {
            panic!("generator region retains its sole iteration");
        };

        let BoundExpression::Generator(iteration_body) = self
            .unit
            .view()
            .expression(*iteration)
            .expect("generator iteration exists")
        else {
            panic!("generator region contains its iteration");
        };

        let syntax = expression.origin().source_anchor().syntax();

        let boolean = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let pair = self
            .context
            .semantic_values()
            .intern_type(TypeData::Tuple(vec![boolean, boolean].into()))?;

        let every = self.boolean_constant(true)?;
        let some = self.boolean_constant(false)?;
        let initial = self.push_node(CheckedTemplateOperation::tuple([every, some]), pair);
        let initial = self.materialize(initial);
        let initial = self.record_evaluation(initial);

        let root = self.storage.identity_entries().find_map(|(root, identity)| {
            matches!(identity, StorageIdentity::Temporary(id) | StorageIdentity::Result(bray_bound_tree::AnyBoundNodeId::Expression(id)) if id == producer).then_some(root)
        }).expect("checked generator owns its result storage");

        self.introduce_state(root, initial);
        self.generator_values.insert(producer, root);

        assert!(
            self.generator_targets.insert(syntax, root).is_none(),
            "generator region has one active production"
        );

        let unit = self
            .types
            .expression(*iteration)
            .expect("generator iteration retains its unit type")
            .ty();

        let operation = self.iteration_loop(
            *iteration,
            iteration_body.pattern(),
            iteration_body.body(),
            unit,
        )?;

        self.generator_targets.remove(&syntax);

        let operation = self.push_node(operation, unit);

        self.record_evaluation(operation);

        let summary = self
            .current_state(root)
            .expect("completed generator retains its reductions");

        let summary = self.materialize(summary);
        let summary = self.record_evaluation(summary);

        self.assign_state(root, summary);

        Ok(summary)
    }

    pub(in crate::compilation::template) fn generator_fold(
        &mut self,
        fold: BoundExpressionId,
        producer: BoundExpressionId,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let summary = self.generator_summary(producer)?;

        let BoundExpression::Structured(expression) = self
            .unit
            .view()
            .expression(fold)
            .expect("selected generator fold exists")
        else {
            panic!("generator observer is a Boolean fold");
        };

        let ordinal = u32::from(expression.kind() == BoundStructuredExpressionKind::BooleanAnyFold);

        let value = self.push_node(
            CheckedTemplateOperation::TupleElement {
                subject: summary,
                index: SymbolOrdinal::new(ordinal),
            },
            ty,
        );

        Ok(CheckedTemplateOperation::Sequence {
            before: summary,
            value,
        })
    }

    pub(in crate::compilation::template) fn generator_yield(
        &mut self,
        root: StorageIdentityId,
        operand: BoundExpressionId,
    ) -> Result<(), FactQueryError> {
        let value = self.expression(operand)?;
        let value = self.materialize(value);
        let value = self.record_evaluation(value);

        let summary = self
            .current_state(root)
            .expect("yield belongs to its eager generator");

        let pair = self.nodes[usize::try_from(summary.raw()).expect("summary node fits host")].ty();
        let boolean = self.nodes[usize::try_from(value.raw()).expect("yield node fits host")].ty();

        let every = self.push_node(
            CheckedTemplateOperation::TupleElement {
                subject: summary,
                index: SymbolOrdinal::new(0),
            },
            boolean,
        );

        let some = self.push_node(
            CheckedTemplateOperation::TupleElement {
                subject: summary,
                index: SymbolOrdinal::new(1),
            },
            boolean,
        );

        // The yielded expression is evaluated before either reduction, even after a decisive result.
        let every = self.push_node(
            CheckedTemplateOperation::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left: every,
                right: value,
            },
            boolean,
        );

        let some = self.push_node(
            CheckedTemplateOperation::Binary {
                operation: ConstantBinaryOperation::LogicalOr,
                left: some,
                right: value,
            },
            boolean,
        );

        let value = self.push_node(CheckedTemplateOperation::tuple([every, some]), pair);

        let updated = self.push_node(
            CheckedTemplateOperation::Update {
                subject: summary,
                base: summary,
                component: summary,
                value,
            },
            pair,
        );

        let updated = self.record_evaluation(updated);

        self.assign_state(root, updated);

        Ok(())
    }
}
