use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnit, CheckedExpressionTypes, ConstructionTarget, SelectedArgument, SelectedCall,
    SelectedConstructionInput, SelectedOperation,
};

/// The exact checked semantic choice attached to one expression occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticSelection {
    /// The exact value selected for a subject-dependent pattern reference.
    Reference(BoundReferenceTarget),
    /// An exact callable, ABI, witness set, and argument mapping.
    Call(SelectedCall),
    /// An exact member, operator, index, construction, conversion, or implementation operation.
    Operation(SelectedOperation),
}

/// One source-correlated expression and its exact semantic selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticSelectionEntry {
    expression: BoundExpressionId,
    selection: SemanticSelection,
}

impl SemanticSelectionEntry {
    /// Creates one expression selection entry.
    pub const fn new(expression: BoundExpressionId, selection: SemanticSelection) -> Self {
        Self {
            expression,
            selection,
        }
    }

    /// Returns the exact expression occurrence.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the selected semantic operation.
    pub const fn selection(&self) -> &SemanticSelection {
        &self.selection
    }
}

/// A malformed checked semantic-selection table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticSelectionTableBuildError {
    /// Expression types describe another bound unit.
    ForeignExpressionTypes,
    /// A selection belongs to another bound unit or names no committed expression.
    InvalidExpression(BoundExpressionId),
    /// More than one selection was supplied for one expression occurrence.
    DuplicateExpression(BoundExpressionId),
    /// A selection category does not match its bound expression.
    SelectionKindMismatch(BoundExpressionId),
    /// A selected value result disagrees with the final checked expression type.
    ResultTypeMismatch(BoundExpressionId),
    /// A selected conversion source disagrees with its bound operand type.
    OperandTypeMismatch(BoundExpressionId),
    /// An implementation requirement disagrees with its bound subject type.
    SubjectTypeMismatch(BoundExpressionId),
}

/// Complete immutable semantic selections for one checked bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedSemanticSelections {
    unit: crate::BoundUnitId,
    kind: crate::BoundUnitKind,
    entries: Arc<[SemanticSelectionEntry]>,
}

impl CheckedSemanticSelections {
    /// Creates one validated table in canonical expression-ID order.
    pub fn try_new(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        entries: impl IntoIterator<Item = SemanticSelectionEntry>,
    ) -> Result<Self, SemanticSelectionTableBuildError> {
        if types.unit() != unit.unit() || types.kind() != unit.key().kind() {
            return Err(SemanticSelectionTableBuildError::ForeignExpressionTypes);
        }

        let mut entries = entries.into_iter().collect::<Vec<_>>();

        for entry in &entries {
            validate_entry(unit, types, entry)?;
        }

        entries.sort_unstable_by_key(SemanticSelectionEntry::expression);

        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(SemanticSelectionTableBuildError::DuplicateExpression(
                pair[0].expression(),
            ));
        }

        Ok(Self {
            unit: unit.unit(),
            kind: unit.key().kind(),
            entries: entries.into(),
        })
    }

    /// Returns the exact bound unit described by these selections.
    pub const fn unit(&self) -> crate::BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the selected bound unit.
    pub const fn kind(&self) -> crate::BoundUnitKind {
        self.kind
    }

    /// Returns selections in canonical expression-ID order.
    pub fn entries(&self) -> &[SemanticSelectionEntry] {
        &self.entries
    }

    /// Returns the exact semantic selection for one expression occurrence.
    pub fn expression(&self, expression: BoundExpressionId) -> Option<&SemanticSelection> {
        self.entries
            .binary_search_by_key(&expression, SemanticSelectionEntry::expression)
            .ok()
            .and_then(|index| self.entries.get(index))
            .map(SemanticSelectionEntry::selection)
    }
}

fn validate_entry(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    entry: &SemanticSelectionEntry,
) -> Result<(), SemanticSelectionTableBuildError> {
    let expression_id = entry.expression();

    let Some(expression) = unit.view().expression(expression_id) else {
        return Err(SemanticSelectionTableBuildError::InvalidExpression(
            expression_id,
        ));
    };

    if !selection_matches_expression(entry.selection(), expression) {
        return Err(SemanticSelectionTableBuildError::SelectionKindMismatch(
            expression_id,
        ));
    }

    validate_operation_subject(types, expression_id, expression, entry.selection())?;

    let selected_type = selection_result_type(entry.selection());

    if let Some(selected_type) = selected_type {
        let Some(expression_type) = types.expression(expression_id) else {
            return Err(SemanticSelectionTableBuildError::InvalidExpression(
                expression_id,
            ));
        };

        if !expression_type.is_recovered() && expression_type.ty() != selected_type {
            return Err(SemanticSelectionTableBuildError::ResultTypeMismatch(
                expression_id,
            ));
        }
    }

    Ok(())
}

fn selection_matches_expression(
    selection: &SemanticSelection,
    expression: &BoundExpression,
) -> bool {
    match (selection, expression) {
        (SemanticSelection::Reference(target), BoundExpression::PatternReference(source)) => {
            *target == BoundReferenceTarget::Local(source.binding().into())
        }
        (SemanticSelection::Call(call), BoundExpression::Call(source)) => {
            call_matches_expression(call, source)
        }
        (SemanticSelection::Operation(operation), expression)
            if operation.matches_expression(expression) =>
        {
            match operation {
                SelectedOperation::Construction(construction) => {
                    construction_matches(construction, expression)
                }
                _ => true,
            }
        }
        _ => false,
    }
}

fn validate_operation_subject(
    types: &CheckedExpressionTypes,
    expression_id: BoundExpressionId,
    expression: &BoundExpression,
    selection: &SemanticSelection,
) -> Result<(), SemanticSelectionTableBuildError> {
    match (selection, expression) {
        (
            SemanticSelection::Operation(SelectedOperation::Conversion(conversion)),
            BoundExpression::Conversion(source),
        ) => validate_subject_type(
            types,
            source.operand(),
            conversion.source_type(),
            expression_id,
            SemanticSelectionTableBuildError::OperandTypeMismatch,
        ),
        (SemanticSelection::Operation(SelectedOperation::Implementation(witness)), _) => {
            validate_subject_type(
                types,
                expression_id,
                witness.requirement().subject(),
                expression_id,
                SemanticSelectionTableBuildError::SubjectTypeMismatch,
            )
        }
        _ => Ok(()),
    }
}

fn validate_subject_type(
    types: &CheckedExpressionTypes,
    subject: BoundExpressionId,
    expected: bray_symbols::TypeId,
    selection: BoundExpressionId,
    mismatch: fn(BoundExpressionId) -> SemanticSelectionTableBuildError,
) -> Result<(), SemanticSelectionTableBuildError> {
    let Some(actual) = types.expression(subject) else {
        return Err(SemanticSelectionTableBuildError::InvalidExpression(subject));
    };

    if !actual.is_recovered() && actual.ty() != expected {
        return Err(mismatch(selection));
    }

    Ok(())
}

fn construction_matches(
    construction: &crate::SelectedConstruction,
    expression: &BoundExpression,
) -> bool {
    let target_matches = match (construction.target(), expression) {
        (ConstructionTarget::Struct(_), BoundExpression::StructConstruction(_))
        | (
            ConstructionTarget::UnionVariant(_),
            BoundExpression::LeadingDotVariant(_)
            | BoundExpression::MemberAccess(_)
            | BoundExpression::Call(_),
        ) => true,
        (ConstructionTarget::TypeForm(_), BoundExpression::Structured(source)) => {
            source.kind() == BoundStructuredExpressionKind::TypeFormConstruction
        }
        _ => false,
    };

    if !target_matches || !construction_inputs_are_valid(construction) {
        return false;
    }

    construction
        .inputs()
        .iter()
        .filter_map(|input| match input {
            SelectedConstructionInput::Explicit { expression, .. } => Some(*expression),
            SelectedConstructionInput::Default { .. } => None,
        })
        .eq(construction_source_inputs(expression))
}

fn call_matches_expression(call: &SelectedCall, source: &crate::BoundCallExpression) -> bool {
    if source
        .resolution()
        .resolved()
        .is_some_and(|resolution| resolution != call.resolution())
    {
        return false;
    }

    let declaration_backed = matches!(call.target(), crate::BoundCallableTarget::Declaration(_));
    let mut parameters = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut saw_default = false;

    if !call.arguments().iter().all(|argument| match argument {
        SelectedArgument::Explicit {
            parameter, ordinal, ..
        } if !saw_default
            && parameter.is_some() == declaration_backed
            && (declaration_backed
                || usize::try_from(*ordinal)
                    .is_ok_and(|ordinal| ordinal < source.arguments().len())) =>
        {
            ordinals.insert(*ordinal)
                && parameter.is_none_or(|parameter| parameters.insert(parameter))
        }
        SelectedArgument::Default { parameter, .. } => {
            saw_default = true;
            declaration_backed && parameters.insert(*parameter)
        }
        SelectedArgument::Explicit { .. } => false,
    }) {
        return false;
    }

    let mut selected_witnesses = call
        .witnesses()
        .iter()
        .map(|witness| witness.witness())
        .collect::<Vec<_>>();

    selected_witnesses.sort_unstable();
    selected_witnesses.dedup();

    selected_witnesses == call.resolution().implementation_witnesses()
        && call
            .arguments()
            .iter()
            .filter_map(|argument| match argument {
                SelectedArgument::Explicit { expression, .. } => Some(*expression),
                SelectedArgument::Default { .. } => None,
            })
            .eq(source
                .arguments()
                .iter()
                .map(crate::BoundArgument::expression))
}

fn construction_inputs_are_valid(construction: &crate::SelectedConstruction) -> bool {
    let mut inputs = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut saw_default = false;

    construction.inputs().iter().all(|input| match input {
        SelectedConstructionInput::Explicit { input, ordinal, .. } if !saw_default => {
            inputs.insert(*input)
                && ordinals.insert(*ordinal)
                && construction.target().accepts_input(*input)
        }
        SelectedConstructionInput::Default {
            input,
            provider,
            ordinal,
        } => {
            saw_default = true;

            inputs.insert(*input)
                && ordinals.insert(*ordinal)
                && construction.target().accepts_input(*input)
                && input.accepts_default(*provider)
        }
        SelectedConstructionInput::Explicit { .. } => false,
    })
}

fn construction_source_inputs(expression: &BoundExpression) -> Vec<BoundExpressionId> {
    match expression {
        BoundExpression::StructConstruction(source) => source
            .fields()
            .iter()
            .map(crate::BoundStructFieldInitializer::expression)
            .collect(),
        BoundExpression::Call(source) => source
            .arguments()
            .iter()
            .map(crate::BoundArgument::expression)
            .collect(),
        BoundExpression::Structured(source)
            if source.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            source.operands().to_vec()
        }
        _ => Vec::new(),
    }
}

fn selection_result_type(selection: &SemanticSelection) -> Option<bray_symbols::TypeId> {
    match selection {
        SemanticSelection::Reference(_) => None,
        SemanticSelection::Call(call) => Some(call.resolution().result().ty()),
        SemanticSelection::Operation(operation) => operation.result_type(),
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::testing::{implementation_instance, implementation_requirement};
    use bray_symbols::{FunctionSymbolId, SymbolId, TraitSymbolId, TypeData};

    use super::{
        CheckedSemanticSelections, SemanticSelection, SemanticSelectionEntry,
        SemanticSelectionTableBuildError,
    };
    use crate::test_support::{expression_unit, push_expression, semantic_values};
    use crate::testing::checked_expression_types;
    use crate::{
        BoundConversionExpression, BoundErrorExpression, BoundExpression, BoundExpressionId,
        BoundMemberAccessExpression, BoundMemberSelector, BoundOperator, BoundUnit, BoundUnitId,
        CheckedExpressionTypes, ConversionTarget, ExpressionTypeEntry, ExpressionTypeResult,
        ExpressionTypeStatus, MemberTarget, OperatorTarget, SelectedConversion,
        SelectedImplementationWitness, SelectedOperation,
    };

    #[test]
    fn semantic_selection_tables_validate_and_index_committed_expressions() {
        let fixture = member_fixture(BoundUnitId::new(70));
        let entry = fixture.entry(fixture.value_type);

        let table = match CheckedSemanticSelections::try_new(
            &fixture.unit,
            &fixture.types,
            [entry.clone()],
        ) {
            Ok(table) => table,
            Err(error) => panic!("valid semantic selection must publish: {error:?}"),
        };

        assert_eq!(table.expression(fixture.member), Some(entry.selection()));
    }

    #[test]
    fn semantic_selection_tables_reject_nonexistent_same_unit_expressions() {
        let fixture = member_fixture(BoundUnitId::new(71));
        let missing = BoundExpressionId::from_slot(fixture.unit.unit(), 99);

        let result = CheckedSemanticSelections::try_new(
            &fixture.unit,
            &fixture.types,
            [SemanticSelectionEntry::new(
                missing,
                fixture.entry(fixture.value_type).selection().clone(),
            )],
        );

        assert_eq!(
            result,
            Err(SemanticSelectionTableBuildError::InvalidExpression(missing))
        );
    }

    #[test]
    fn semantic_selection_tables_reject_wrong_categories_and_result_types() {
        let fixture = member_fixture(BoundUnitId::new(72));

        let wrong_category = SemanticSelectionEntry::new(
            fixture.member,
            SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type: fixture.value_type,
            }),
        );

        assert_eq!(
            CheckedSemanticSelections::try_new(&fixture.unit, &fixture.types, [wrong_category]),
            Err(SemanticSelectionTableBuildError::SelectionKindMismatch(
                fixture.member
            ))
        );

        let result = CheckedSemanticSelections::try_new(
            &fixture.unit,
            &fixture.types,
            [fixture.entry(fixture.other_type)],
        );

        assert_eq!(
            result,
            Err(SemanticSelectionTableBuildError::ResultTypeMismatch(
                fixture.member
            ))
        );
    }

    #[test]
    fn semantic_selection_tables_reject_duplicate_expression_entries() {
        let fixture = member_fixture(BoundUnitId::new(73));
        let entry = fixture.entry(fixture.value_type);

        let result = CheckedSemanticSelections::try_new(
            &fixture.unit,
            &fixture.types,
            [entry.clone(), entry],
        );

        assert_eq!(
            result,
            Err(SemanticSelectionTableBuildError::DuplicateExpression(
                fixture.member
            ))
        );
    }

    #[test]
    fn semantic_selection_tables_reject_conversion_operand_type_mismatches() {
        let values = semantic_values();
        let source_type = intern_type(&values, TypeData::tuple([]));
        let target_type = intern_type(&values, TypeData::tuple([source_type]));

        let (unit, expressions) = expression_unit(BoundUnitId::new(74), |tree, origin| {
            let operand = push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, source_type)),
            );

            let conversion = push_expression(
                tree,
                BoundExpression::Conversion(BoundConversionExpression::new(
                    origin,
                    operand,
                    origin.source_anchor().syntax(),
                    Some(target_type),
                    Some(target_type),
                    false,
                )),
            );

            vec![operand, conversion]
        });

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [
                ExpressionTypeEntry::new(
                    expressions[0],
                    ExpressionTypeResult::new(source_type, ExpressionTypeStatus::Valid),
                ),
                ExpressionTypeEntry::new(
                    expressions[1],
                    ExpressionTypeResult::new(target_type, ExpressionTypeStatus::Valid),
                ),
            ],
        );

        let selection = SelectedOperation::Conversion(SelectedConversion::new(
            target_type,
            target_type,
            ConversionTarget::Identity,
        ));

        let entry =
            SemanticSelectionEntry::new(expressions[1], SemanticSelection::Operation(selection));

        assert_eq!(
            CheckedSemanticSelections::try_new(&unit, &types, [entry]),
            Err(SemanticSelectionTableBuildError::OperandTypeMismatch(
                expressions[1]
            ))
        );
    }

    #[test]
    fn semantic_selection_tables_reject_implementation_subject_type_mismatches() {
        let values = semantic_values();
        let subject_type = intern_type(&values, TypeData::tuple([]));
        let other_type = intern_type(&values, TypeData::tuple([subject_type]));
        let trait_definition = TraitSymbolId::from_symbol_id(SymbolId::new(20));

        let requirement =
            implementation_requirement(&values, trait_definition, other_type, subject_type);

        let witness = implementation_instance(&values, 21);

        let (unit, expressions) = expression_unit(BoundUnitId::new(75), |tree, origin| {
            let expression = push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, subject_type)),
            );

            vec![expression]
        });

        let result = ExpressionTypeResult::new(subject_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let entry = SemanticSelectionEntry::new(
            expressions[0],
            SemanticSelection::Operation(SelectedOperation::Implementation(
                SelectedImplementationWitness::new(requirement, witness),
            )),
        );

        assert_eq!(
            CheckedSemanticSelections::try_new(&unit, &types, [entry]),
            Err(SemanticSelectionTableBuildError::SubjectTypeMismatch(
                expressions[0]
            ))
        );
    }

    struct MemberFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        member: BoundExpressionId,
        value_type: bray_symbols::TypeId,
        other_type: bray_symbols::TypeId,
    }

    impl MemberFixture {
        fn entry(&self, result_type: bray_symbols::TypeId) -> SemanticSelectionEntry {
            let member = FunctionSymbolId::from_symbol_id(SymbolId::new(4));

            SemanticSelectionEntry::new(
                self.member,
                SemanticSelection::Operation(SelectedOperation::Member(MemberTarget::new(
                    member.into(),
                    result_type,
                    [],
                ))),
            )
        }
    }

    fn member_fixture(unit: BoundUnitId) -> MemberFixture {
        let values = semantic_values();

        let value_type = match values.intern_type(TypeData::tuple([])) {
            Ok(ty) => ty,
            Err(error) => panic!("test value type must be valid: {error:?}"),
        };

        let other_type = match values.intern_type(TypeData::tuple([value_type])) {
            Ok(ty) => ty,
            Err(error) => panic!("test alternate type must be valid: {error:?}"),
        };

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let receiver = push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, value_type)),
            );

            let member = push_expression(
                tree,
                BoundExpression::MemberAccess(BoundMemberAccessExpression::new(
                    origin,
                    receiver,
                    Some(BoundMemberSelector::Name(test_name())),
                    None,
                    false,
                )),
            );

            vec![receiver, member]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        MemberFixture {
            member: expressions[1],
            unit,
            types,
            value_type,
            other_type,
        }
    }

    fn test_name() -> bray_symbols::SymbolName {
        let Some(name) = bray_symbols::SymbolName::try_new("value") else {
            panic!("test member name must be valid");
        };

        name
    }

    fn intern_type(
        values: &bray_symbols::SemanticValueStore,
        data: TypeData,
    ) -> bray_symbols::TypeId {
        match values.intern_type(data) {
            Ok(ty) => ty,
            Err(error) => panic!("test type must be valid: {error:?}"),
        }
    }
}
