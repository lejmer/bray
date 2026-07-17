use std::collections::BTreeSet;

use super::{
    CheckedConversion, CheckedExpressionFactBuildError, CheckedExpressionFactEntry,
    CheckedExpressionFactInput, CheckedExpressionFactKind, CheckedExpressionFacts,
    CheckedExpressionResult, CheckedOperatorTarget,
};
use crate::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    BoundUnitRoot, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, walk_bound_tree,
};

impl CheckedExpressionFacts {
    /// Validates and publishes every expression fact required by value lowering.
    pub fn try_new(
        unit: &BoundUnit,
        input: CheckedExpressionFactInput,
    ) -> Result<Self, CheckedExpressionFactBuildError> {
        let reachable = reachable_expressions(unit)?;
        let results = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Result,
            input.results,
        )?;
        let literals = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Literal,
            input.literals,
        )?;
        let operators = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Operator,
            input.operators,
        )?;
        let members = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Member,
            input.members,
        )?;
        let indexes = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Index,
            input.indexes,
        )?;
        let constructions = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Construction,
            input.constructions,
        )?;
        let conversions = canonical_entries(
            unit,
            &reachable,
            CheckedExpressionFactKind::Conversion,
            input.conversions,
        )?;

        let facts = Self::from_canonical(
            unit.unit(),
            unit.key().kind(),
            CheckedExpressionFactInput {
                results,
                literals,
                operators,
                members,
                indexes,
                constructions,
                conversions,
            },
        );

        facts.validate_complete(unit, &reachable)?;

        Ok(facts)
    }

    fn validate_complete(
        &self,
        unit: &BoundUnit,
        reachable: &BTreeSet<BoundExpressionId>,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        for &expression in reachable {
            let Some(result) = self.result(expression) else {
                return Err(CheckedExpressionFactBuildError::MissingResult(expression));
            };

            let Some(bound) = unit.tree().expression(expression) else {
                return Err(CheckedExpressionFactBuildError::MissingExpression {
                    kind: CheckedExpressionFactKind::Result,
                    expression,
                });
            };

            if bound.is_recovered() && !result.is_recovered() {
                return Err(
                    CheckedExpressionFactBuildError::RecoveredExpressionMarkedValid(expression),
                );
            }

            if bound.ty().is_some_and(|ty| ty != result.ty()) {
                return Err(CheckedExpressionFactBuildError::InconsistentFact {
                    kind: CheckedExpressionFactKind::Result,
                    expression,
                });
            }

            if result.is_recovered() {
                self.validate_present_fact(expression, bound, result)?;
            } else {
                self.validate_required_fact(expression, bound, result)?;
            }
        }

        self.validate_categories(unit)
    }

    fn validate_required_fact(
        &self,
        expression: BoundExpressionId,
        bound: &BoundExpression,
        result: CheckedExpressionResult,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        if let BoundExpression::Call(call) = bound {
            if call.resolution().resolved().is_none() {
                return Err(CheckedExpressionFactBuildError::MissingRequiredFact {
                    kind: CheckedExpressionFactKind::Invocation,
                    expression,
                });
            }

            return self.validate_present_fact(expression, bound, result);
        }

        let Some(kind) = required_fact_kind(bound) else {
            return Ok(());
        };

        if !self.has_fact(kind, expression) {
            return Err(CheckedExpressionFactBuildError::MissingRequiredFact { kind, expression });
        }

        self.validate_present_fact(expression, bound, result)
    }

    fn validate_present_fact(
        &self,
        expression: BoundExpressionId,
        bound: &BoundExpression,
        result: CheckedExpressionResult,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        if let BoundExpression::Call(call) = bound {
            return if call.resolution().resolved().is_some() {
                self.validate_call(expression, call, result)
            } else {
                Ok(())
            };
        }

        let Some(kind) = required_fact_kind(bound) else {
            return Ok(());
        };

        match bound {
            BoundExpression::Conversion(conversion) => {
                let Some(checked) = self.conversion(expression) else {
                    return Ok(());
                };

                let source = self.result(conversion.operand()).map(|result| result.ty());

                if source != Some(checked.source())
                    || checked.target() != result.ty()
                    || conversion
                        .target_type()
                        .is_some_and(|target| target != checked.target())
                    || !checked.is_structurally_valid()
                {
                    return Err(CheckedExpressionFactBuildError::InconsistentFact {
                        kind,
                        expression,
                    });
                }
            }
            BoundExpression::Unary(operation) => {
                self.validate_operator(expression, kind, operation.operator())?
            }
            BoundExpression::Binary(operation) => {
                self.validate_operator(expression, kind, operation.operator())?
            }
            BoundExpression::StructConstruction(construction) => self.validate_construction(
                expression,
                kind,
                construction.fields().iter().map(|field| field.expression()),
            )?,
            BoundExpression::LeadingDotVariant(_) => {
                self.validate_construction(expression, kind, std::iter::empty())?
            }
            BoundExpression::Structured(construction) => self.validate_construction(
                expression,
                kind,
                construction.operands().iter().copied(),
            )?,
            _ => {}
        }

        Ok(())
    }

    fn validate_call(
        &self,
        expression: BoundExpressionId,
        call: &crate::BoundCallExpression,
        result: CheckedExpressionResult,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        let Some(resolved) = call.resolution().resolved() else {
            return Err(CheckedExpressionFactBuildError::MissingRequiredFact {
                kind: CheckedExpressionFactKind::Invocation,
                expression,
            });
        };

        let mapped: Vec<_> = resolved
            .arguments()
            .explicit()
            .iter()
            .map(|argument| argument.expression())
            .collect();
        let source: Vec<_> = call
            .arguments()
            .iter()
            .map(|argument| argument.expression())
            .collect();

        if resolved.result().ty() != result.ty()
            || !resolved
                .arguments()
                .is_valid_for(resolved.callable(), self.unit())
            || resolved
                .arguments()
                .receiver()
                .is_some_and(|receiver| self.result(receiver.expression()).is_none())
            || mapped != source
            || resolved.arguments().explicit().iter().any(|argument| {
                !self.conversion_matches_expression(argument.expression(), argument.conversion())
            })
        {
            return Err(CheckedExpressionFactBuildError::InconsistentFact {
                kind: CheckedExpressionFactKind::Invocation,
                expression,
            });
        }

        Ok(())
    }

    fn validate_operator(
        &self,
        expression: BoundExpressionId,
        kind: CheckedExpressionFactKind,
        source_operator: crate::BoundOperator,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        let Some(selection) = self.operator(expression) else {
            return Ok(());
        };

        if matches!(
            selection.target(),
            CheckedOperatorTarget::BuiltIn(operator) if operator != source_operator
        ) {
            return Err(CheckedExpressionFactBuildError::InconsistentFact { kind, expression });
        }

        Ok(())
    }

    fn validate_construction(
        &self,
        expression: BoundExpressionId,
        kind: CheckedExpressionFactKind,
        explicit: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Result<(), CheckedExpressionFactBuildError> {
        let Some(construction) = self.construction(expression) else {
            return Ok(());
        };

        if construction.explicit_expressions().ne(explicit)
            || construction
                .explicit_inputs()
                .any(|(source, conversion)| !self.conversion_matches_expression(source, conversion))
        {
            return Err(CheckedExpressionFactBuildError::InconsistentFact { kind, expression });
        }

        Ok(())
    }

    fn conversion_matches_expression(
        &self,
        expression: BoundExpressionId,
        conversion: &CheckedConversion,
    ) -> bool {
        self.result(expression)
            .is_some_and(|result| result.ty() == conversion.source())
            && conversion.is_structurally_valid()
    }

    fn validate_categories(&self, unit: &BoundUnit) -> Result<(), CheckedExpressionFactBuildError> {
        validate_category(unit, CheckedExpressionFactKind::Literal, &self.literals)?;
        validate_category(unit, CheckedExpressionFactKind::Operator, &self.operators)?;
        validate_category(unit, CheckedExpressionFactKind::Member, &self.members)?;
        validate_category(unit, CheckedExpressionFactKind::Index, &self.indexes)?;
        validate_category(
            unit,
            CheckedExpressionFactKind::Construction,
            &self.constructions,
        )?;
        validate_category(
            unit,
            CheckedExpressionFactKind::Conversion,
            &self.conversions,
        )?;

        for entry in &*self.constructions {
            if !entry.value().is_valid_for(unit.unit()) {
                return Err(CheckedExpressionFactBuildError::InconsistentFact {
                    kind: CheckedExpressionFactKind::Construction,
                    expression: entry.expression(),
                });
            }
        }

        Ok(())
    }

    fn has_fact(&self, kind: CheckedExpressionFactKind, expression: BoundExpressionId) -> bool {
        match kind {
            CheckedExpressionFactKind::Result => false,
            CheckedExpressionFactKind::Literal => self.literal(expression).is_some(),
            CheckedExpressionFactKind::Operator => self.operator(expression).is_some(),
            CheckedExpressionFactKind::Member => self.member(expression).is_some(),
            CheckedExpressionFactKind::Index => self.index(expression).is_some(),
            CheckedExpressionFactKind::Construction => self.construction(expression).is_some(),
            CheckedExpressionFactKind::Invocation => false,
            CheckedExpressionFactKind::Conversion => self.conversion(expression).is_some(),
        }
    }
}

fn canonical_entries<T>(
    unit: &BoundUnit,
    reachable: &BTreeSet<BoundExpressionId>,
    kind: CheckedExpressionFactKind,
    entries: impl IntoIterator<Item = CheckedExpressionFactEntry<T>>,
) -> Result<Vec<CheckedExpressionFactEntry<T>>, CheckedExpressionFactBuildError> {
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by_key(CheckedExpressionFactEntry::expression);

    let mut previous = None;

    for entry in &entries {
        let expression = entry.expression();

        if expression.unit() != unit.unit() {
            return Err(CheckedExpressionFactBuildError::ForeignExpression { kind, expression });
        }

        if unit.tree().expression(expression).is_none() {
            return Err(CheckedExpressionFactBuildError::MissingExpression { kind, expression });
        }

        if !reachable.contains(&expression) {
            return Err(CheckedExpressionFactBuildError::UnreachableFact { kind, expression });
        }

        if previous == Some(expression) {
            return Err(CheckedExpressionFactBuildError::DuplicateFact { kind, expression });
        }

        previous = Some(expression);
    }

    Ok(entries)
}

fn reachable_expressions(
    unit: &BoundUnit,
) -> Result<BTreeSet<BoundExpressionId>, CheckedExpressionFactBuildError> {
    let mut expressions = BTreeSet::new();
    let outcome = walk_bound_tree(unit.tree(), root_node(unit.root()), |event| {
        if let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event {
            expressions.insert(expression);
        }

        BoundWalkControl::Continue
    });

    match outcome {
        BoundWalkOutcome::Completed => Ok(expressions),
        BoundWalkOutcome::MissingNode(node) => {
            Err(CheckedExpressionFactBuildError::MissingBoundNode(node))
        }
        BoundWalkOutcome::Stopped => Err(CheckedExpressionFactBuildError::TraversalStopped),
    }
}

const fn root_node(root: BoundUnitRoot) -> AnyBoundNodeId {
    match root {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            AnyBoundNodeId::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
    }
}

fn validate_category<T>(
    unit: &BoundUnit,
    kind: CheckedExpressionFactKind,
    entries: &[CheckedExpressionFactEntry<T>],
) -> Result<(), CheckedExpressionFactBuildError> {
    for entry in entries {
        let Some(expression) = unit.tree().expression(entry.expression()) else {
            return Err(CheckedExpressionFactBuildError::MissingExpression {
                kind,
                expression: entry.expression(),
            });
        };

        if required_fact_kind(expression) != Some(kind) {
            return Err(CheckedExpressionFactBuildError::WrongExpressionCategory {
                kind,
                expression: entry.expression(),
            });
        }
    }

    Ok(())
}

const fn required_fact_kind(expression: &BoundExpression) -> Option<CheckedExpressionFactKind> {
    match expression {
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => {
            Some(CheckedExpressionFactKind::Operator)
        }
        BoundExpression::Conversion(_) => Some(CheckedExpressionFactKind::Conversion),
        BoundExpression::StructConstruction(_) | BoundExpression::LeadingDotVariant(_) => {
            Some(CheckedExpressionFactKind::Construction)
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            Some(CheckedExpressionFactKind::Member)
        }
        BoundExpression::Structured(expression) => match expression.kind() {
            BoundStructuredExpressionKind::Literal => Some(CheckedExpressionFactKind::Literal),
            BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex => Some(CheckedExpressionFactKind::Index),
            BoundStructuredExpressionKind::TypeFormConstruction => {
                Some(CheckedExpressionFactKind::Construction)
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;
    use bray_symbols::{
        CallableAbi, CallableDefinitionId, CallableInstanceData,
        CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, ConstantValueData,
        ConstantValueKind, FunctionSymbolId, GenericArgument, GenericOwnerId,
        GenericParameterSymbolId, LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey,
        LocalSymbolRegionRole, LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, SemanticValueStore,
        StructFieldSymbolId, StructSymbolId, SymbolId, SymbolKind, SymbolName, SymbolOrdinal,
        TypeData,
    };

    use super::{
        CheckedExpressionFactBuildError, CheckedExpressionFactEntry, CheckedExpressionFactInput,
        CheckedExpressionFactKind, CheckedExpressionFacts, CheckedExpressionResult,
    };
    use crate::{
        BoundArgument, BoundBlock, BoundBlockItem, BoundCallExpression, BoundCallResult,
        BoundCallableBody, BoundConversionExpression, BoundErrorExpression, BoundExpression,
        BoundExpressionId, BoundMemberAccessExpression, BoundMemberSelector, BoundNameExpression,
        BoundNodeOrigin, BoundOperator, BoundReferenceTarget, BoundResolvedCall,
        BoundStructConstructionExpression, BoundStructFieldInitializer, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTree, BoundTreeBuilder, BoundUnaryExpression,
        BoundUnit, BoundUnitId, BoundUnitKey, BoundUnitRoot, CheckedArgumentMapping,
        CheckedCallableReference, CheckedCallableTarget, CheckedConstruction,
        CheckedConstructionInput, CheckedConstructionInputTarget, CheckedConstructionInputValue,
        CheckedConstructionTarget, CheckedConversion, CheckedConversionKind,
        CheckedDefaultArgument, CheckedExplicitArgument, CheckedExpressionStatus,
        CheckedIndexSelection, CheckedIndexTarget, CheckedLiteral, CheckedMemberSelection,
        CheckedMemberTarget, CheckedOperatorSelection, CheckedOperatorTarget,
        CheckedParameterTarget,
    };

    #[test]
    fn publication_orders_complete_literal_facts_by_expression_identity() {
        let values = semantic_values();
        let ty = error_type(&values);
        let literal_value = literal_value(&values, ty);
        let unit_id = BoundUnitId::new(4);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let mut tree = BoundTreeBuilder::new(unit_id);

        let first = push_expression(&mut tree, literal_expression(origin, ty));
        let second = push_expression(&mut tree, literal_expression(origin, ty));
        let root = push_callable_root(&mut tree, origin, [first, second]);
        let unit = callable_unit(unit_id, tree.finish(), root);

        let facts = match CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([
                result(second, ty, CheckedExpressionStatus::Valid),
                result(first, ty, CheckedExpressionStatus::Valid),
            ])
            .with_literals([
                CheckedExpressionFactEntry::new(second, CheckedLiteral::new(literal_value)),
                CheckedExpressionFactEntry::new(first, CheckedLiteral::new(literal_value)),
            ]),
        ) {
            Ok(facts) => facts,
            Err(error) => panic!("complete literal facts must publish: {error:?}"),
        };

        assert_eq!(facts.results()[0].expression(), first);
        assert_eq!(facts.results()[1].expression(), second);
        assert_eq!(
            facts.literal(first),
            Some(CheckedLiteral::new(literal_value))
        );
        assert!(!facts.is_recovered());
    }

    #[test]
    fn publication_rejects_results_that_disagree_with_bound_node_types() {
        let values = semantic_values();
        let bound_type = error_type(&values);
        let result_type = tuple_type(&values);
        let literal_value = literal_value(&values, bound_type);
        let unit_id = BoundUnitId::new(5);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let mut tree = BoundTreeBuilder::new(unit_id);
        let literal = push_expression(&mut tree, literal_expression(origin, bound_type));
        let root = push_callable_root(&mut tree, origin, [literal]);
        let unit = callable_unit(unit_id, tree.finish(), root);

        let facts = CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([result(
                literal,
                result_type,
                CheckedExpressionStatus::Valid,
            )])
            .with_literals([CheckedExpressionFactEntry::new(
                literal,
                CheckedLiteral::new(literal_value),
            )]),
        );

        assert_eq!(
            facts,
            Err(CheckedExpressionFactBuildError::InconsistentFact {
                kind: CheckedExpressionFactKind::Result,
                expression: literal,
            })
        );
    }

    #[test]
    fn publication_rejects_facts_for_committed_unreachable_expressions() {
        let values = semantic_values();
        let ty = error_type(&values);
        let literal_value = literal_value(&values, ty);
        let unit_id = BoundUnitId::new(6);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let mut tree = BoundTreeBuilder::new(unit_id);
        let reachable = push_expression(&mut tree, literal_expression(origin, ty));
        let unreachable = push_expression(&mut tree, literal_expression(origin, ty));
        let root = push_callable_root(&mut tree, origin, [reachable]);
        let unit = callable_unit(unit_id, tree.finish(), root);

        let facts = CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([
                result(reachable, ty, CheckedExpressionStatus::Valid),
                result(unreachable, ty, CheckedExpressionStatus::Valid),
            ])
            .with_literals([
                CheckedExpressionFactEntry::new(reachable, CheckedLiteral::new(literal_value)),
                CheckedExpressionFactEntry::new(unreachable, CheckedLiteral::new(literal_value)),
            ]),
        );

        assert_eq!(
            facts,
            Err(CheckedExpressionFactBuildError::UnreachableFact {
                kind: CheckedExpressionFactKind::Result,
                expression: unreachable,
            })
        );
    }

    #[test]
    fn publication_retains_every_exact_value_lowering_fact_family() {
        let values = semantic_values();
        let ty = error_type(&values);
        let literal_value = literal_value(&values, ty);
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(20));
        let field = StructFieldSymbolId::from_symbol_id(SymbolId::new(21));
        let Some(member_name) = SymbolName::try_new("value") else {
            panic!("member name must be valid");
        };
        let unit_id = BoundUnitId::new(8);
        let anchor = crate::test_support::source_anchor();
        let origin = BoundNodeOrigin::source(anchor);
        let mut tree = BoundTreeBuilder::new(unit_id);

        let literal = push_expression(&mut tree, literal_expression(origin, ty));
        let unary = push_expression(
            &mut tree,
            BoundExpression::Unary(BoundUnaryExpression::new(
                origin,
                BoundOperator::Add,
                [literal],
                Some(ty),
                false,
            )),
        );
        let conversion = push_expression(
            &mut tree,
            BoundExpression::Conversion(BoundConversionExpression::new(
                origin,
                unary,
                anchor.syntax(),
                Some(ty),
                Some(ty),
                false,
            )),
        );
        let member = push_expression(
            &mut tree,
            BoundExpression::MemberAccess(BoundMemberAccessExpression::new(
                origin,
                literal,
                Some(BoundMemberSelector::Name(member_name)),
                Some(ty),
                false,
            )),
        );
        let index = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::ElementIndex,
                [literal, literal],
                [],
                [],
                Some(ty),
                false,
            )),
        );
        let construction = push_expression(
            &mut tree,
            BoundExpression::StructConstruction(BoundStructConstructionExpression::new(
                origin,
                None,
                [BoundStructFieldInitializer::new(None, literal, false)],
                Some(ty),
                false,
            )),
        );
        let root = push_callable_root(&mut tree, origin, [conversion, member, index, construction]);
        let unit = callable_unit(unit_id, tree.finish(), root);
        let checked_construction = match CheckedConstruction::try_new(
            CheckedConstructionTarget::Struct(structure),
            [CheckedConstructionInput::new(
                CheckedConstructionInputTarget::StructField(field),
                CheckedConstructionInputValue::Explicit {
                    expression: literal,
                    conversion: CheckedConversion::new(ty, ty, CheckedConversionKind::Identity),
                },
            )],
        ) {
            Ok(construction) => construction,
            Err(error) => panic!("construction input must be valid: {error:?}"),
        };

        let facts = match CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([
                result(literal, ty, CheckedExpressionStatus::Valid),
                result(unary, ty, CheckedExpressionStatus::Valid),
                result(conversion, ty, CheckedExpressionStatus::Valid),
                result(member, ty, CheckedExpressionStatus::Valid),
                result(index, ty, CheckedExpressionStatus::Valid),
                result(construction, ty, CheckedExpressionStatus::Valid),
            ])
            .with_literals([CheckedExpressionFactEntry::new(
                literal,
                CheckedLiteral::new(literal_value),
            )])
            .with_operators([CheckedExpressionFactEntry::new(
                unary,
                CheckedOperatorSelection::new(CheckedOperatorTarget::BuiltIn(BoundOperator::Add)),
            )])
            .with_members([CheckedExpressionFactEntry::new(
                member,
                CheckedMemberSelection::new(CheckedMemberTarget::Declaration(field.into()), []),
            )])
            .with_indexes([CheckedExpressionFactEntry::new(
                index,
                CheckedIndexSelection::new(CheckedIndexTarget::ArrayElement),
            )])
            .with_constructions([CheckedExpressionFactEntry::new(
                construction,
                checked_construction.clone(),
            )])
            .with_conversions([CheckedExpressionFactEntry::new(
                conversion,
                CheckedConversion::new(ty, ty, CheckedConversionKind::Identity),
            )]),
        ) {
            Ok(facts) => facts,
            Err(error) => panic!("complete expression facts must publish: {error:?}"),
        };

        assert_eq!(
            facts.literal(literal),
            Some(CheckedLiteral::new(literal_value))
        );
        assert_eq!(
            facts.operator(unary),
            Some(CheckedOperatorSelection::new(
                CheckedOperatorTarget::BuiltIn(BoundOperator::Add)
            ))
        );
        assert_eq!(
            facts.member(member).map(CheckedMemberSelection::target),
            Some(CheckedMemberTarget::Declaration(field.into()))
        );
        assert_eq!(
            facts.index(index).map(CheckedIndexSelection::target),
            Some(&CheckedIndexTarget::ArrayElement)
        );
        assert_eq!(
            facts.construction(construction),
            Some(&checked_construction)
        );
        assert_eq!(
            facts.conversion(conversion).map(CheckedConversion::kind),
            Some(&CheckedConversionKind::Identity)
        );
    }

    #[test]
    fn valid_literals_require_values_while_error_nodes_allow_recovery() {
        let values = semantic_values();
        let ty = error_type(&values);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let unit_id = BoundUnitId::new(5);
        let mut literal_tree = BoundTreeBuilder::new(unit_id);
        let literal = push_expression(&mut literal_tree, literal_expression(origin, ty));
        let literal_root = push_callable_root(&mut literal_tree, origin, [literal]);
        let literal_unit = callable_unit(unit_id, literal_tree.finish(), literal_root);

        let missing = CheckedExpressionFacts::try_new(
            &literal_unit,
            CheckedExpressionFactInput::new([result(literal, ty, CheckedExpressionStatus::Valid)]),
        );

        assert_eq!(
            missing,
            Err(CheckedExpressionFactBuildError::MissingRequiredFact {
                kind: CheckedExpressionFactKind::Literal,
                expression: literal,
            })
        );

        let duplicate = CheckedExpressionFacts::try_new(
            &literal_unit,
            CheckedExpressionFactInput::new([
                result(literal, ty, CheckedExpressionStatus::Valid),
                result(literal, ty, CheckedExpressionStatus::Valid),
            ]),
        );

        assert_eq!(
            duplicate,
            Err(CheckedExpressionFactBuildError::DuplicateFact {
                kind: CheckedExpressionFactKind::Result,
                expression: literal,
            })
        );

        let foreign = BoundExpressionId::from_slot(BoundUnitId::new(99), 0);
        let foreign_result = CheckedExpressionFacts::try_new(
            &literal_unit,
            CheckedExpressionFactInput::new([result(foreign, ty, CheckedExpressionStatus::Valid)]),
        );

        assert_eq!(
            foreign_result,
            Err(CheckedExpressionFactBuildError::ForeignExpression {
                kind: CheckedExpressionFactKind::Result,
                expression: foreign,
            })
        );

        let recovered_id = BoundUnitId::new(6);
        let mut recovered_tree = BoundTreeBuilder::new(recovered_id);
        let recovered = push_expression(
            &mut recovered_tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, ty)),
        );
        let recovered_root = push_callable_root(&mut recovered_tree, origin, [recovered]);
        let recovered_unit = callable_unit(recovered_id, recovered_tree.finish(), recovered_root);
        let facts = match CheckedExpressionFacts::try_new(
            &recovered_unit,
            CheckedExpressionFactInput::new([result(
                recovered,
                ty,
                CheckedExpressionStatus::Recovered,
            )]),
        ) {
            Ok(facts) => facts,
            Err(error) => panic!("recovery facts must remain publishable: {error:?}"),
        };

        assert!(facts.is_recovered());
    }

    #[test]
    fn recovered_expressions_still_reject_inconsistent_present_facts() {
        let values = semantic_values();
        let source = error_type(&values);
        let target = tuple_type(&values);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let unit_id = BoundUnitId::new(7);
        let mut tree = BoundTreeBuilder::new(unit_id);
        let operand = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, source)),
        );
        let conversion = push_expression(
            &mut tree,
            BoundExpression::Conversion(BoundConversionExpression::new(
                origin,
                operand,
                origin.source_anchor().syntax(),
                Some(target),
                Some(target),
                true,
            )),
        );
        let root = push_callable_root(&mut tree, origin, [conversion]);
        let unit = callable_unit(unit_id, tree.finish(), root);

        let facts = CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([
                result(operand, source, CheckedExpressionStatus::Recovered),
                result(conversion, target, CheckedExpressionStatus::Recovered),
            ])
            .with_conversions([CheckedExpressionFactEntry::new(
                conversion,
                CheckedConversion::new(source, source, CheckedConversionKind::Identity),
            )]),
        );

        assert_eq!(
            facts,
            Err(CheckedExpressionFactBuildError::InconsistentFact {
                kind: CheckedExpressionFactKind::Conversion,
                expression: conversion,
            })
        );
    }

    #[test]
    fn invocations_retain_exact_callable_abi_argument_mapping_and_defaults() {
        let values = semantic_values();
        let ty = error_type(&values);
        let literal_value = literal_value(&values, ty);
        let definition = FunctionSymbolId::from_symbol_id(SymbolId::new(11));
        let instance = callable_instance(&values, definition);
        let first_parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(12));
        let second_parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(13));
        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(14));
        let unit_id = BoundUnitId::new(7);
        let origin = BoundNodeOrigin::source(crate::test_support::source_anchor());
        let mut tree = BoundTreeBuilder::new(unit_id);

        let callee = push_expression(
            &mut tree,
            BoundExpression::Name(BoundNameExpression::new(
                origin,
                BoundReferenceTarget::Surface(definition.into()),
                Some(ty),
                false,
            )),
        );
        let argument = push_expression(&mut tree, literal_expression(origin, ty));
        let mapping = match CheckedArgumentMapping::try_new(
            CallableAbi::C,
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared {
                    callable: instance,
                    parameter: first_parameter,
                    ordinal: SymbolOrdinal::new(0),
                },
                argument,
                CheckedConversion::new(ty, ty, CheckedConversionKind::Identity),
            )],
            [CheckedDefaultArgument::new(
                instance,
                second_parameter,
                provider,
                SymbolOrdinal::new(1),
                None,
            )],
        ) {
            Ok(mapping) => mapping,
            Err(error) => panic!("distinct argument mapping must be valid: {error:?}"),
        };
        let callable =
            CheckedCallableReference::new(CheckedCallableTarget::Direct(instance), CallableAbi::C);
        let call = push_expression(
            &mut tree,
            BoundExpression::Call(BoundCallExpression::resolved(
                origin,
                callee,
                [BoundArgument::new(argument, None, false)],
                BoundResolvedCall::new(callable, [], mapping, BoundCallResult::Immediate(ty)),
            )),
        );
        let root = push_callable_root(&mut tree, origin, [call]);
        let unit = callable_unit(unit_id, tree.finish(), root);

        let facts = match CheckedExpressionFacts::try_new(
            &unit,
            CheckedExpressionFactInput::new([
                result(callee, ty, CheckedExpressionStatus::Valid),
                result(argument, ty, CheckedExpressionStatus::Valid),
                result(call, ty, CheckedExpressionStatus::Valid),
            ])
            .with_literals([CheckedExpressionFactEntry::new(
                argument,
                CheckedLiteral::new(literal_value),
            )]),
        ) {
            Ok(facts) => facts,
            Err(error) => panic!("complete invocation facts must publish: {error:?}"),
        };

        let Some(BoundExpression::Call(call)) = unit.tree().expression(call) else {
            panic!("call expression must remain in the canonical bound unit");
        };
        let Some(invocation) = call.resolution().resolved() else {
            panic!("call expression must retain its invocation fact");
        };

        assert_eq!(
            facts.result(call.callee()),
            Some(CheckedExpressionResult::new(
                ty,
                CheckedExpressionStatus::Valid,
            ))
        );
        assert_eq!(invocation.callable(), callable);
        assert_eq!(invocation.arguments().explicit()[0].expression(), argument);
        assert_eq!(
            invocation.arguments().explicit()[0].conversion().kind(),
            &CheckedConversionKind::Identity
        );
        assert_eq!(invocation.arguments().defaults()[0].provider(), provider);
    }

    fn literal_expression(origin: BoundNodeOrigin, ty: bray_symbols::TypeId) -> BoundExpression {
        BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Literal,
            [],
            [],
            [],
            Some(ty),
            false,
        ))
    }

    fn result(
        expression: BoundExpressionId,
        ty: bray_symbols::TypeId,
        status: CheckedExpressionStatus,
    ) -> CheckedExpressionFactEntry<CheckedExpressionResult> {
        CheckedExpressionFactEntry::new(expression, CheckedExpressionResult::new(ty, status))
    }

    fn push_expression(
        tree: &mut BoundTreeBuilder,
        expression: BoundExpression,
    ) -> BoundExpressionId {
        match tree.push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test expression must be valid: {error:?}"),
        }
    }

    fn push_callable_root(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
    ) -> crate::BoundCallableBodyId {
        let block = match tree.push_block(BoundBlock::new(
            origin,
            expressions.into_iter().map(BoundBlockItem::Expression),
            false,
        )) {
            Ok(block) => block,
            Err(error) => panic!("test block must be valid: {error:?}"),
        };

        match tree.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(root) => root,
            Err(error) => panic!("test callable root must be valid: {error:?}"),
        }
    }

    fn callable_unit(
        unit: BoundUnitId,
        tree: BoundTree,
        root: crate::BoundCallableBodyId,
    ) -> BoundUnit {
        let owner = crate::test_support::symbol_key(SymbolKind::Function, unit.raw());
        let Some(key) =
            BoundUnitKey::callable_body(owner.clone(), crate::test_support::source_anchor())
        else {
            panic!("function must support a callable body");
        };
        let locals = local_snapshot(unit, owner);

        match BoundUnit::try_new(key, tree, locals, [], BoundUnitRoot::CallableBody(root)) {
            Ok(unit) => unit,
            Err(error) => panic!("test bound unit must be valid: {error:?}"),
        }
    }

    fn local_snapshot(unit: BoundUnitId, owner: bray_symbols::SymbolKey) -> LocalSymbolSnapshot {
        let anchor = crate::test_support::source_anchor().syntax();
        let Some(key) = LocalSymbolRegionKey::try_new(
            owner,
            LocalSymbolRegionRole::CallableBody,
            [anchor],
            None,
        ) else {
            panic!("test local region key must be valid");
        };
        let mut builder =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit.raw()), key);

        if let Err(error) =
            builder.push_scope(None, LocalScopeBoundary::Root, anchor, TextSize::ZERO)
        {
            panic!("test root scope must be valid: {error:?}");
        }

        match builder.finish() {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test local snapshot must be valid: {error:?}"),
        }
    }

    fn semantic_values() -> SemanticValueStore {
        match SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("test semantic store must be available: {error:?}"),
        }
    }

    fn error_type(values: &SemanticValueStore) -> bray_symbols::TypeId {
        match values.intern_type(TypeData::Error) {
            Ok(ty) => ty,
            Err(error) => panic!("test error type must be interned: {error:?}"),
        }
    }

    fn tuple_type(values: &SemanticValueStore) -> bray_symbols::TypeId {
        crate::test_support::tuple_type_in(values)
    }

    fn literal_value(
        values: &SemanticValueStore,
        ty: bray_symbols::TypeId,
    ) -> bray_symbols::ConstantValueId {
        match values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
        {
            Ok(value) => value,
            Err(error) => panic!("test literal value must be interned: {error:?}"),
        }
    }

    fn callable_instance(
        values: &SemanticValueStore,
        definition: FunctionSymbolId,
    ) -> bray_symbols::CallableInstanceId {
        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            panic!("function must support a generic substitution");
        };
        let substitution = match bray_symbols::GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        ) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty substitution must be valid: {error:?}"),
        };
        let substitution = match values.intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty substitution must be interned: {error:?}"),
        };
        let Some(definition) = CallableDefinitionId::try_new(definition.into()) else {
            panic!("function must be a callable definition");
        };

        match values.intern_callable_instance(CallableInstanceData::new(definition, substitution)) {
            Ok(instance) => instance,
            Err(error) => panic!("callable instance must be interned: {error:?}"),
        }
    }
}
