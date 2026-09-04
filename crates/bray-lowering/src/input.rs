// rust-style: allow(module-too-large, reason = "lowering inputs and their cross-input validation form one cohesive boundary contract")

use bray_bound_tree::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, BoundUnitId, BoundUnitKind, CheckedBodyBehavior,
    CheckedControlFlow, CheckedDependencyContracts, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedPatterns, CheckedRefinements, CheckedSemanticSelections, Liveness,
    RefinementKind, StorageAccessPlan, StorageAccessPurpose, StorageFlow, StorageOperationDecision,
    StoragePlan,
};
use bray_ir::{MirTargetContract, MirUnitBuilder, MirUnitKind};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, ConstantValueId, SemanticValueStore,
    SemanticValueStoreError,
};

use crate::plan::dependency_subject_exists;
use crate::result::requires_mir;
use crate::{LoweringPlanFailure, VerifiedLoweringPlans};

/// A validated borrowed view of the completed checked HIR required by lowering.
///
/// The view keeps the canonical bound unit and its associated semantic analysis
/// separate. Adding another required checker domain extends this input rather than creating a
/// copied or progressively wrapped bound-tree representation.
#[derive(Clone)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    control_flow: &'unit CheckedControlFlow,
    expression_types: &'unit CheckedExpressionTypes,
    patterns: &'unit CheckedPatterns,
    literal_values: &'unit CheckedLiteralValues,
    liveness: &'unit Liveness,
    refinements: &'unit CheckedRefinements,
    lowering_plans: VerifiedLoweringPlans<'unit>,
    body_behavior: &'unit CheckedBodyBehavior,
    semantic_values: &'unit SemanticValueStore,
    constant_reference_values: &'unit [(BoundExpressionId, ConstantValueId)],
    native_static_templates: &'unit [bray_symbols::StaticInstanceTemplateId],
    static_owner: Option<(bray_symbols::StaticReferenceSelection, bray_symbols::TypeId)>,
    unit_kind: MirUnitKind,
    target: MirTargetContract,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic input belongs to the exact bound unit.
    #[expect(
        clippy::too_many_arguments,
        reason = "each canonical lowering input remains an independently requestable input"
    )]
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlow,
        expression_types: &'unit CheckedExpressionTypes,
        patterns: &'unit CheckedPatterns,
        literal_values: &'unit CheckedLiteralValues,
        liveness: &'unit Liveness,
        refinements: &'unit CheckedRefinements,
        lowering_plans: VerifiedLoweringPlans<'unit>,
        body_behavior: &'unit CheckedBodyBehavior,
        semantic_values: &'unit SemanticValueStore,
        unit_kind: MirUnitKind,
        target: MirTargetContract,
    ) -> Result<Self, LoweringInputError> {
        if !requires_mir(unit.key()) {
            return Err(LoweringInputError::CompileTimeUnitRequiresClassification);
        }

        validate_input_owner(
            unit,
            control_flow.unit(),
            control_flow.kind(),
            LoweringInputKind::ControlFlow,
        )?;

        validate_input_owner(
            unit,
            expression_types.unit(),
            expression_types.kind(),
            LoweringInputKind::ExpressionTypes,
        )?;

        validate_input_owner(
            unit,
            patterns.unit(),
            patterns.kind(),
            LoweringInputKind::Patterns,
        )?;

        validate_input_owner(
            unit,
            literal_values.unit(),
            literal_values.kind(),
            LoweringInputKind::LiteralValues,
        )?;

        validate_input_owner(
            unit,
            liveness.unit(),
            liveness.kind(),
            LoweringInputKind::Liveness,
        )?;

        validate_input_owner(
            unit,
            refinements.unit(),
            refinements.kind(),
            LoweringInputKind::Refinements,
        )?;

        validate_input_owner(
            unit,
            lowering_plans.unit(),
            lowering_plans.kind(),
            LoweringInputKind::LoweringPlans,
        )?;

        validate_input_owner(
            unit,
            body_behavior.unit(),
            body_behavior.kind(),
            LoweringInputKind::BodyBehavior,
        )?;

        validate_literal_target(literal_values, &target)?;
        validate_literal_values(literal_values, semantic_values)?;

        let semantic_selections = lowering_plans.semantic_selections();
        let storage_plan = lowering_plans.storage_plan();
        let storage_flow = lowering_plans.storage_flow();

        validate_semantic_completeness(unit, expression_types, semantic_selections, storage_plan)?;

        validate_pattern_completeness(unit, patterns)?;

        validate_liveness(unit, storage_plan, liveness)?;
        validate_refinements(unit, storage_plan, refinements)?;
        validate_storage_analysis(unit, storage_plan, storage_flow)?;

        if lowering_plans.runtime_abi() != target.runtime_abi() {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::LoweringPlans,
            ));
        }

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            expression_types,
            patterns,
            literal_values,
            liveness,
            refinements,
            lowering_plans,
            body_behavior,
            semantic_values,
            constant_reference_values: &[],
            native_static_templates: &[],
            static_owner: None,
            unit_kind,
            target,
        })
    }

    /// Returns the canonical checked source-shaped semantic unit.
    pub const fn unit(&self) -> &'unit BoundUnit {
        self.unit
    }

    /// Returns the durable control-flow analysis established for the unit.
    pub const fn control_flow(&self) -> &'unit CheckedControlFlow {
        self.control_flow
    }

    /// Returns final types for every bound expression occurrence.
    pub const fn expression_types(&self) -> &'unit CheckedExpressionTypes {
        self.expression_types
    }

    /// Returns checked pattern operations, binding types, and match coverage.
    pub const fn patterns(&self) -> &'unit CheckedPatterns {
        self.patterns
    }

    /// Returns exact semantic choices for the unit.
    pub const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.lowering_plans.semantic_selections()
    }

    /// Returns source literals adapted to their final checked types.
    pub const fn literal_values(&self) -> &'unit CheckedLiteralValues {
        self.literal_values
    }

    /// Returns exact storage identities, relationships, and evaluated accesses.
    pub const fn storage_plan(&self) -> &'unit StoragePlan {
        self.lowering_plans.storage_plan()
    }

    /// Returns durable last-use and lexical lifetime decisions.
    pub const fn liveness(&self) -> &'unit Liveness {
        self.liveness
    }

    /// Returns flow-sensitive analysis available at checked operation occurrences.
    pub const fn refinements(&self) -> &'unit CheckedRefinements {
        self.refinements
    }

    /// Returns checked ownership, movement, and borrow decisions.
    pub const fn storage_flow(&self) -> &'unit StorageFlow {
        self.lowering_plans.storage_flow()
    }

    /// Returns normalized dependency contracts for unit-local semantic occurrences.
    pub const fn dependency_contracts(&self) -> &'unit CheckedDependencyContracts {
        self.lowering_plans.dependency_contracts()
    }

    /// Returns complete checked plans verified for direct lowering consumption.
    pub const fn lowering_plans(&self) -> &VerifiedLoweringPlans<'unit> {
        &self.lowering_plans
    }

    /// Returns normalized effects, capabilities, and lifecycle obligations.
    pub const fn body_behavior(&self) -> &'unit CheckedBodyBehavior {
        self.body_behavior
    }

    /// Returns the canonical semantic values referenced by checked analysis.
    pub const fn semantic_values(&self) -> &'unit SemanticValueStore {
        self.semantic_values
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub const fn available_compiler_known_symbols(&self) -> &'unit AvailableCompilerKnownSymbols {
        self.lowering_plans.available_compiler_known_symbols()
    }

    /// Adds closed constant values reached through source references.
    pub fn with_constant_reference_values(
        mut self,
        values: &'unit [(BoundExpressionId, ConstantValueId)],
    ) -> Result<Self, LoweringInputError> {
        validate_constant_reference_values(
            self.unit,
            self.expression_types,
            self.semantic_values,
            values,
        )?;

        self.constant_reference_values = values;

        Ok(self)
    }

    /// Adds the open self realization carried by one static initializer template.
    pub fn with_static_owner(
        mut self,
        reference: bray_symbols::StaticReferenceSelection,
        ty: bray_symbols::TypeId,
    ) -> Self {
        self.static_owner = Some((reference, ty));

        self
    }

    /// Adds declarations whose source references produce provider-owned native addresses.
    pub fn with_native_static_templates(
        mut self,
        templates: &'unit [bray_symbols::StaticInstanceTemplateId],
    ) -> Self {
        self.native_static_templates = templates;

        self
    }

    /// Returns whether one selected static reference produces a native storage address.
    pub fn is_native_static(&self, template: bray_symbols::StaticInstanceTemplateId) -> bool {
        self.native_static_templates.contains(&template)
    }

    /// Returns the open self realization carried by a static initializer template.
    pub const fn static_owner(
        &self,
    ) -> Option<&(bray_symbols::StaticReferenceSelection, bray_symbols::TypeId)> {
        self.static_owner.as_ref()
    }

    /// Returns the closed value reached by one constant reference occurrence.
    pub fn constant_reference_value(
        &self,
        expression: BoundExpressionId,
    ) -> Option<ConstantValueId> {
        self.constant_reference_values
            .binary_search_by_key(&expression, |(expression, _)| *expression)
            .ok()
            .map(|index| self.constant_reference_values[index].1)
    }

    /// Returns the MIR representation category selected for this source unit.
    pub const fn unit_kind(&self) -> &MirUnitKind {
        &self.unit_kind
    }

    /// Returns target analysis selected for lowering this unit.
    pub const fn target(&self) -> &MirTargetContract {
        &self.target
    }

    /// Creates the canonical source-unit MIR builder for this input.
    pub fn mir_builder(&self) -> MirUnitBuilder {
        // The lowerer retains its validated input while the builder owns the immutable target analysis.
        MirUnitBuilder::for_bound(
            self.unit.identity(),
            self.unit_kind.clone(),
            self.target.clone(),
        )
    }
}

/// A semantic input required by source-unit lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoweringInputKind {
    /// Control-flow analysis.
    ControlFlow,
    /// Final expression types.
    ExpressionTypes,
    /// Pattern operations, binding types, and match coverage.
    Patterns,
    /// Source-literal values.
    LiteralValues,
    /// Closed values reached through source constant references.
    ConstantReferences,
    /// Last-use and lexical lifetime decisions.
    Liveness,
    /// Flow-sensitive semantic refinements.
    Refinements,
    /// Verified async, task, cleanup, and runtime lowering plans.
    LoweringPlans,
    /// Effects, capabilities, execution requirements, and lifecycle obligations.
    BodyBehavior,
}

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LoweringInputError {
    /// A required semantic input belongs to another bound unit.
    ForeignInput {
        /// The required input category.
        input: LoweringInputKind,
        /// The bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied input.
        actual: BoundUnitId,
    },
    /// A required semantic input describes another unit category.
    InputKindMismatch {
        /// The required input category.
        input: LoweringInputKind,
        /// The category carried by the bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied input.
        actual: BoundUnitKind,
    },
    /// A successfully typed expression lacks the semantic choice required by lowering.
    MissingSemanticSelection(BoundExpressionId),
    /// A bound expression has no final checked type.
    MissingExpressionType(BoundExpressionId),
    /// Pattern analysis does not cover the bound patterns, bindings, and matches exactly.
    InvalidPatternInput,
    /// One input contains identities absent from its exact bound unit or dependent input.
    InvalidInputContents(LoweringInputKind),
    /// A semantic value required to validate the lowering input could not be read.
    SemanticValue(SemanticValueStoreError),
    /// A checked storage operation does not match the canonical storage plan.
    InvalidStorageOperation(BoundExpressionId),
    /// Checked storage operations do not cover every canonical access plan exactly once.
    StorageOperationCountMismatch {
        /// The number of canonical access plans.
        expected: usize,
        /// The number of checked operation decisions.
        actual: usize,
    },
    /// A scope-exit storage decision references an unknown scope, identity, access, or borrow.
    InvalidStorageExit(BoundBlockId),
    /// Async, cleanup, task, or runtime analyses cannot form one complete lowering plan.
    InvalidPlan(Box<LoweringPlanFailure>),
    /// Literal adaptation used a machine-sized integer width from another target.
    LiteralTargetWidthMismatch {
        /// The width required by the lowering target.
        expected: std::num::NonZeroU16,
        /// The width used while adapting source literals.
        actual: std::num::NonZeroU16,
    },
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
    /// A compile-time-only unit was supplied through executable MIR lowering.
    CompileTimeUnitRequiresClassification,
}

impl From<SemanticValueStoreError> for LoweringInputError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}

impl From<LoweringPlanFailure> for LoweringInputError {
    fn from(error: LoweringPlanFailure) -> Self {
        Self::InvalidPlan(Box::new(error))
    }
}

fn validate_literal_target(
    literals: &CheckedLiteralValues,
    target: &MirTargetContract,
) -> Result<(), LoweringInputError> {
    let expected = target.machine().pointer_width_bits();
    let actual = literals.target_integer_width_bits();

    if actual != expected {
        return Err(LoweringInputError::LiteralTargetWidthMismatch { expected, actual });
    }

    Ok(())
}

fn validate_literal_values(
    literals: &CheckedLiteralValues,
    values: &SemanticValueStore,
) -> Result<(), LoweringInputError> {
    for entry in literals.entries() {
        values.constant_value_data(entry.value())?;
    }

    Ok(())
}

fn validate_constant_reference_values(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    values: &SemanticValueStore,
    references: &[(BoundExpressionId, ConstantValueId)],
) -> Result<(), LoweringInputError> {
    if references.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::ConstantReferences,
        ));
    }

    for (expression, value) in references {
        let Some(BoundExpression::Name(name)) = unit.view().expression(*expression) else {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        };

        if !matches!(
            name.target(),
            BoundReferenceTarget::Surface(
                AnySymbolId::Constant(_)
                    | AnySymbolId::TraitConstantMember(_)
                    | AnySymbolId::TraitConstantFulfillment(_)
            )
        ) {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        }

        let Some(expression_type) = types.expression(*expression) else {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        };

        let value_type = values.constant_value_data(*value)?.ty();

        if expression_type.ty() != value_type {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        }
    }

    Ok(())
}

fn validate_semantic_completeness(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
) -> Result<(), LoweringInputError> {
    for (expression, node) in unit.tree().expressions() {
        if !requires_expression_type(node) {
            continue;
        }

        let Some(result) = types.expression(expression) else {
            return Err(LoweringInputError::MissingExpressionType(expression));
        };

        if result.is_recovered() {
            continue;
        }

        if requires_semantic_selection(node, expression, storage)
            && selections.expression(expression).is_none()
        {
            return Err(LoweringInputError::MissingSemanticSelection(expression));
        }
    }

    Ok(())
}

const fn requires_expression_type(expression: &BoundExpression) -> bool {
    match expression {
        BoundExpression::Name(expression) => expression.ty().is_some(),
        BoundExpression::MemberAccess(expression) => expression.ty().is_some(),
        _ => true,
    }
}

fn validate_pattern_completeness(
    unit: &BoundUnit,
    analysis: &CheckedPatterns,
) -> Result<(), LoweringInputError> {
    let patterns_match = unit
        .tree()
        .patterns()
        .map(|(pattern, _)| pattern)
        .eq(analysis.patterns().iter().map(|entry| entry.pattern()));

    let bindings_match = patterns_match && {
        let mut introduced = Vec::new();

        for ((_, pattern), checked) in unit.tree().patterns().zip(analysis.patterns()) {
            introduced.extend(checked.bindings(pattern));
        }

        introduced.sort_unstable();
        introduced.dedup();

        introduced
            .into_iter()
            .eq(analysis.binding_types().iter().map(|entry| entry.binding()))
    };

    let matches_match = unit
        .tree()
        .expressions()
        .filter_map(|(expression, node)| {
            matches!(node, BoundExpression::Match(_)).then_some(expression)
        })
        .eq(analysis.matches().iter().map(|entry| entry.expression()));

    if !patterns_match || !bindings_match || !matches_match {
        return Err(LoweringInputError::InvalidPatternInput);
    }

    Ok(())
}

fn validate_liveness(
    unit: &BoundUnit,
    storage: &StoragePlan,
    analysis: &Liveness,
) -> Result<(), LoweringInputError> {
    let valid_last_uses = analysis.last_uses().iter().all(|entry| {
        unit.view().node_is_recovered(entry.operation()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    let valid_scopes = analysis.live_across_scopes().iter().all(|entry| {
        unit.view().block(entry.scope()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    let valid_suspensions = analysis.live_across_suspensions().iter().all(|entry| {
        unit.view().expression(entry.await_expression()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    if !valid_last_uses || !valid_scopes || !valid_suspensions {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::Liveness,
        ));
    }

    Ok(())
}

fn validate_refinements(
    unit: &BoundUnit,
    storage: &StoragePlan,
    analysis: &CheckedRefinements,
) -> Result<(), LoweringInputError> {
    let valid = analysis.occurrences().iter().all(|occurrence| {
        unit.view().node_is_recovered(occurrence.node()).is_some()
            && occurrence.refinements().iter().all(|input| {
                refinement_kind_exists(unit, storage, input.kind())
                    && input
                        .dependencies()
                        .iter()
                        .all(|access| storage.access(*access).is_some())
            })
    });

    if !valid {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::Refinements,
        ));
    }

    Ok(())
}

fn refinement_kind_exists(unit: &BoundUnit, storage: &StoragePlan, kind: RefinementKind) -> bool {
    match kind {
        RefinementKind::Condition { expression, .. }
        | RefinementKind::NullablePresence { expression, .. }
        | RefinementKind::TrustBoundary(expression)
        | RefinementKind::NormalCompletion(expression) => {
            unit.view().expression(expression).is_some()
        }
        RefinementKind::Pattern {
            subject,
            pattern,
            access,
            ..
        } => {
            unit.view().expression(subject).is_some()
                && unit.view().pattern(pattern).is_some()
                && storage.access(access).is_some()
                && matches!(storage.binding(bray_bound_tree::StorageBindingTarget::PatternSubject(pattern)),
                Some(bray_bound_tree::StorageBinding::Access(original))
                    if storage.relationship(original, access) == bray_bound_tree::StorageRelationship::Identical)
        }
    }
}

fn validate_storage_analysis(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlow,
) -> Result<(), LoweringInputError> {
    let plans = storage.access_plans();
    let decisions = flow.operations();

    if plans.len() != decisions.len() {
        return Err(LoweringInputError::StorageOperationCountMismatch {
            expected: plans.len(),
            actual: decisions.len(),
        });
    }

    for (plan, decision) in plans.iter().copied().zip(decisions.iter().copied()) {
        let plan_matches = plan.expression() == decision.expression()
            && plan.access() == decision.access()
            && plan.purpose().matches_checked(decision.purpose());

        let has_expression = unit.view().expression(decision.expression()).is_some();

        let has_access = storage.access(decision.access()).is_some();

        let borrow_matches = storage_borrow_matches(storage, plan, decision);

        if !plan_matches || !has_expression || !has_access || !borrow_matches {
            return Err(LoweringInputError::InvalidStorageOperation(
                decision.expression(),
            ));
        }
    }

    for exit in flow.exits() {
        let has_scope = unit.view().block(exit.scope()).is_some();

        let has_identities = exit
            .initialized()
            .iter()
            .all(|identity| storage.identity(*identity).is_some());

        let has_accesses = exit
            .moved()
            .iter()
            .all(|access| storage.access(*access).is_some());

        let has_borrows = exit
            .active_borrows()
            .iter()
            .all(|borrow| storage.borrow_capability(*borrow).is_some());

        if !has_scope || !has_identities || !has_accesses || !has_borrows {
            return Err(LoweringInputError::InvalidStorageExit(exit.scope()));
        }
    }

    Ok(())
}

fn storage_borrow_matches(
    storage: &StoragePlan,
    plan: StorageAccessPlan,
    decision: StorageOperationDecision,
) -> bool {
    let StorageAccessPurpose::Borrow(kind) = plan.purpose() else {
        return decision.borrow().is_none();
    };

    let inherited = storage
        .access(plan.access())
        .and_then(|access| access.root().borrow_capability())
        .filter(|borrow| {
            storage
                .borrow_capability(*borrow)
                .is_some_and(|capability| capability.kind() == kind)
        });

    let created = storage
        .borrow_capability_entries()
        .find(|(_, capability)| {
            capability.expression() == Some(plan.expression())
                && capability.kind() == kind
                && capability.access() == plan.access()
        })
        .map(|(borrow, _)| borrow);

    decision.borrow() == created.or(inherited)
}

fn requires_semantic_selection(
    expression: &BoundExpression,
    id: BoundExpressionId,
    storage: &StoragePlan,
) -> bool {
    match expression {
        BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Call(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_)
        | BoundExpression::For(_)
        | BoundExpression::Generator(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::TraitQualifiedMember(_)
        | BoundExpression::PatternReference(_) => true,
        BoundExpression::MemberAccess(expression) => {
            !matches!(
                expression.selector(),
                Some(bray_bound_tree::BoundMemberSelector::TupleElement(_))
            ) && storage.expression_plans(id).next().is_none()
        }
        BoundExpression::Structured(expression) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex
                | BoundStructuredExpressionKind::SliceIndex
                | BoundStructuredExpressionKind::TypeFormConstruction
                | BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold
                | BoundStructuredExpressionKind::NullablePropagation
                | BoundStructuredExpressionKind::ResultPropagation
        ),
        BoundExpression::Block(_)
        | BoundExpression::Literal(_)
        | BoundExpression::Name(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::Assignment(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::AnonymousCallable(_)
        | BoundExpression::Await(_)
        | BoundExpression::ControlTransfer(_)
        | BoundExpression::Match(_)
        | BoundExpression::Error(_) => false,
    }
}

fn validate_input_owner(
    unit: &BoundUnit,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
    input: LoweringInputKind,
) -> Result<(), LoweringInputError> {
    if actual_unit != unit.unit() {
        return Err(LoweringInputError::ForeignInput {
            input,
            expected: unit.unit(),
            actual: actual_unit,
        });
    }

    if actual_kind != unit.key().kind() {
        return Err(LoweringInputError::InputKindMismatch {
            input,
            expected: unit.key().kind(),
            actual: actual_kind,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_bound_tree::{
        AsyncScopeExitPlan, BorrowCapabilityOrigin, BoundBlock, BoundBlockExpression,
        BoundBlockItem, BoundConversionExpression, BoundDependencyContract,
        BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
        BoundExpression, BoundExpressionId, BoundLiteralExpression, BoundLiteralKind,
        BoundNodeOrigin, BoundSourceAnchor, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitRoot,
        CheckedAsync, CheckedBodyBehavior, CheckedControlFlow, CheckedDependencyContracts,
        CheckedExpressionTypes, CheckedLiteralValueEntry, CheckedLiteralValues, CheckedPatterns,
        CheckedRefinements, CheckedSemanticSelections, ControlCompletion, ExpressionTypeEntry,
        ExpressionTypeResult, ExpressionTypeStatus, LastUse, Liveness, PlannedBorrowCapability,
        StorageAccess, StorageAccessId, StorageAccessPurpose, StorageAccessRoot,
        StorageExitDecision, StorageFlow, StorageIdentity, StorageIdentityId,
        StorageOperationDecision, StorageOperationStatus, StoragePlanBuilder,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{
        BorrowKind, ConstantValueData, ConstantValueKind, CurrentRunCancellation,
        SemanticValueStore, SemanticValueStoreError, TypeData, TypeId,
    };
    use bray_testing::{
        test_bound_unit, test_constant_template_unit, test_mir_target, test_runtime_default_unit,
    };

    use super::{LoweringInput, LoweringInputError, LoweringInputKind, validate_literal_values};
    use crate::{
        LoweringPlanFailureCause, LoweringPlanKind, VerifiedLoweringPlans,
    };

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_analysis() {
        let unit = test_bound_unit(4);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let analysis = empty_expression_inputs(&unit);

        let input = match lowering_input(&unit, &control_flow, (&analysis).into()) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
        assert!(std::ptr::eq(input.expression_types(), &analysis.types));
        assert!(std::ptr::eq(input.patterns(), &analysis.patterns));

        assert!(std::ptr::eq(
            input.semantic_selections(),
            &analysis.selections
        ));

        assert!(std::ptr::eq(input.literal_values(), &analysis.literals));
        assert!(std::ptr::eq(input.storage_plan(), &analysis.storage));
        assert!(std::ptr::eq(input.liveness(), &analysis.liveness));
        assert!(std::ptr::eq(input.refinements(), &analysis.refinements));
        assert!(std::ptr::eq(input.storage_flow(), &analysis.storage_flow));

        assert!(std::ptr::eq(
            input.dependency_contracts(),
            &analysis.dependencies
        ));

        assert!(input.lowering_plans().frame_dependencies().is_empty());

        assert!(std::ptr::eq(input.body_behavior(), &analysis.behavior));
        assert!(std::ptr::eq(input.semantic_values(), &analysis.values));

        assert!(std::ptr::eq(
            input.available_compiler_known_symbols(),
            available_compiler_known_symbols()
        ));
    }

    #[test]
    fn input_rejects_compile_time_only_units_before_runtime_input_validation() {
        let unit = test_constant_template_unit(5, push_unit_expression);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let analysis = empty_expression_inputs(&unit);

        assert_input_error(
            lowering_input(&unit, &control_flow, (&analysis).into()),
            LoweringInputError::CompileTimeUnitRequiresClassification,
        );
    }

    #[test]
    fn input_rejects_foreign_and_wrong_category_side_analysis() {
        let unit = test_bound_unit(4);
        let analysis = empty_expression_inputs(&unit);

        let foreign = CheckedControlFlow::new(
            BoundUnitId::new(5),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            lowering_input(&unit, &foreign, (&analysis).into()),
            LoweringInputError::ForeignInput {
                input: LoweringInputKind::ControlFlow,
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(5),
            },
        );

        let wrong_kind = CheckedControlFlow::new(
            unit.unit(),
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
            ControlCompletion::default(),
        );

        assert_input_error(
            lowering_input(&unit, &wrong_kind, (&analysis).into()),
            LoweringInputError::InputKindMismatch {
                input: LoweringInputKind::ControlFlow,
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
            },
        );
    }

    #[test]
    fn input_rejects_expression_analysis_from_another_unit() {
        let unit = test_bound_unit(4);
        let foreign_unit = test_bound_unit(5);
        let local = empty_expression_inputs(&unit);
        let foreign = empty_expression_inputs(&foreign_unit);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let foreign_types = ExpressionInputReferences {
            types: &foreign.types,
            ..(&local).into()
        };

        assert_input_error(
            lowering_input(&unit, &control_flow, foreign_types),
            LoweringInputError::ForeignInput {
                input: LoweringInputKind::ExpressionTypes,
                expected: unit.unit(),
                actual: foreign_unit.unit(),
            },
        );

        let foreign_patterns = ExpressionInputReferences {
            patterns: &foreign.patterns,
            ..(&local).into()
        };

        assert_input_error(
            lowering_input(&unit, &control_flow, foreign_patterns),
            LoweringInputError::ForeignInput {
                input: LoweringInputKind::Patterns,
                expected: unit.unit(),
                actual: foreign_unit.unit(),
            },
        );
    }

    #[test]
    fn input_rejects_verified_plans_for_another_unit() {
        let unit = test_bound_unit(6);
        let foreign_unit = test_bound_unit(7);
        let local = empty_expression_inputs(&unit);
        let foreign = empty_expression_inputs(&foreign_unit);
        let target = test_mir_target();

        let foreign_plans = VerifiedLoweringPlans::try_new(
            &foreign_unit,
            &foreign.storage,
            &foreign.storage_flow,
            &foreign.dependencies,
            &foreign.selections,
            available_compiler_known_symbols(),
            &foreign.async_analysis,
            target.runtime_abi(),
        )
        .unwrap_or_else(|error| panic!("foreign lowering plans must verify: {error:?}"));

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &local.types,
                &local.patterns,
                &local.literals,
                &local.liveness,
                &local.refinements,
                foreign_plans,
                &local.behavior,
                &local.values,
                bray_ir::MirUnitKind::Synchronous,
                target,
            ),
            LoweringInputError::ForeignInput {
                input: LoweringInputKind::LoweringPlans,
                expected: unit.unit(),
                actual: foreign_unit.unit(),
            },
        );
    }

    #[test]
    fn input_rejects_literal_values_adapted_for_another_target_width() {
        let unit = test_bound_unit(8);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let expected = test_mir_target().machine().pointer_width_bits();

        let actual = if expected.get() == 32 {
            NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)
        } else {
            NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN)
        };

        let analysis = empty_expression_inputs_with_width(&unit, actual);

        assert_input_error(
            lowering_input(&unit, &control_flow, (&analysis).into()),
            LoweringInputError::LiteralTargetWidthMismatch { expected, actual },
        );
    }

    #[test]
    fn literal_validation_preserves_a_foreign_semantic_value_id() {
        let first_store = semantic_values();
        let second_store = semantic_values();

        let ty = first_store
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let value = first_store
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
            .unwrap_or_else(|error| panic!("test value must intern: {error:?}"));

        let unit = test_runtime_default_unit(36, |tree, origin| {
            tree.push_expression(BoundExpression::Literal(BoundLiteralExpression::new(
                origin,
                origin.source_anchor().syntax().full_range(),
                BoundLiteralKind::Boolean,
                Some(ty),
                false,
            )))
            .unwrap_or_else(|error| panic!("test literal must fit: {error:?}"))
        });

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test literal unit must retain its root");
        };

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [ExpressionTypeEntry::new(
                expression,
                ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid),
            )],
        );

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &first_store,
            test_mir_target().machine().pointer_width_bits(),
            [CheckedLiteralValueEntry::new(expression, value)],
        )
        .unwrap_or_else(|error| panic!("test literal values must validate: {error:?}"));

        assert_eq!(
            validate_literal_values(&literals, &second_store),
            Err(LoweringInputError::SemanticValue(
                SemanticValueStoreError::ForeignId {
                    expected: second_store.id(),
                    actual: first_store.id(),
                }
            ))
        );
    }

    #[test]
    fn input_rejects_same_unit_liveness_for_another_storage_plan() {
        let (unit, expression, reached_type) = storage_expression_unit(9);

        let mut analysis = empty_expression_inputs(&unit);

        analysis.types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [ExpressionTypeEntry::new(
                expression,
                ExpressionTypeResult::new(reached_type, ExpressionTypeStatus::Valid),
            )],
        );

        analysis.selections = CheckedSemanticSelections::try_new(&unit, &analysis.types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        analysis.literals = CheckedLiteralValues::try_new(
            &unit,
            &analysis.types,
            &semantic_values(),
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("literal-free values must validate: {error:?}"));

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut alternate_storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = alternate_storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("alternate identity must validate: {error:?}"));

        let access = push_storage_access(
            &mut alternate_storage,
            identity,
            reached_type,
            bound.origin().source_anchor(),
        );

        let liveness = Liveness::try_new(
            unit.unit(),
            unit.key().kind(),
            [LastUse::new(
                BoundDependencySubject::StorageAccess(access),
                expression.into(),
            )],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("same-unit liveness must build: {error:?}"));

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let analysis = ExpressionInputReferences {
            liveness: &liveness,
            ..(&analysis).into()
        };

        assert_input_error(
            lowering_input(&unit, &control_flow, analysis),
            LoweringInputError::InvalidInputContents(LoweringInputKind::Liveness),
        );
    }

    #[test]
    fn verified_plans_reject_an_unexpected_suspension() {
        let (unit, expression, reached_type) = storage_expression_unit(10);

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test identity must validate: {error:?}"));

        let access = push_storage_access(
            &mut storage,
            identity,
            reached_type,
            bound.origin().source_anchor(),
        );

        let storage = storage.finish();
        let empty = BoundDependencyContract::new([]);

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            [(expression, empty.clone())],
            [],
            [(access, empty.clone())],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("complete dependencies must validate: {error:?}"));

        let access_contract = BoundDependencyContract::new([BoundDependencyRequirement::direct(
            BoundDependencySubject::StorageAccess(access),
            BoundDependencyRequirementKind::StorageAlive,
        )]);

        let alternate = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            [(expression, empty)],
            [],
            [(access, access_contract)],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("alternate dependencies must validate: {error:?}"));

        let contract = alternate
            .access(access)
            .unwrap_or_else(|| panic!("alternate access contract must exist"));

        let async_analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [bray_bound_tree::AsyncSuspensionPoint::new(
                expression,
                bray_bound_tree::AsyncSuspensionKind::Await {
                    operand: expression,
                },
                Some(contract),
                [],
                [],
                false,
            )],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("same-unit async analysis must build: {error:?}"));

        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must build: {error:?}"));

        let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
            .unwrap_or_else(|error| panic!("empty storage flow must build: {error:?}"));

        let target = test_mir_target();

        let result = VerifiedLoweringPlans::try_new(
            &unit,
            &storage,
            &flow,
            &dependencies,
            &selections,
            available_compiler_known_symbols(),
            &async_analysis,
            target.runtime_abi(),
        );

        let Err(error) = result else {
            panic!("unexpected suspension must fail plan verification");
        };

        assert_eq!(error.kind(), LoweringPlanKind::Suspension);
        assert_eq!(error.cause(), LoweringPlanFailureCause::Unexpected);
        assert_eq!(error.expression(), Some(expression));
    }

    #[test]
    fn async_cleanup_plans_cover_exact_storage_exit_identities() {
        let mut scope = None;
        let mut exit = None;
        let mut other_exit = None;

        let unit = test_runtime_default_unit(27, |tree, origin| {
            let expression = push_unit_expression(tree, origin);

            let block = tree
                .push_block(BoundBlock::new(
                    origin,
                    [BoundBlockItem::Expression(expression)],
                    false,
                ))
                .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

            let block_expression = tree
                .push_expression(BoundExpression::Block(BoundBlockExpression::new(
                    origin, block, None, false,
                )))
                .unwrap_or_else(|error| panic!("test block expression must fit: {error:?}"));

            scope = Some(block);
            exit = Some(expression.into());
            other_exit = Some(block_expression.into());

            block_expression
        });

        let scope = scope.unwrap_or_else(|| panic!("test scope must be captured"));
        let exit = exit.unwrap_or_else(|| panic!("test exit must be captured"));
        let other_exit = other_exit.unwrap_or_else(|| panic!("alternate exit must be captured"));
        let kind = unit.key().kind();

        let storage_flow = StorageFlow::try_new(
            unit.unit(),
            kind,
            [],
            [],
            [StorageExitDecision::new(scope, exit, [], [], [], [], false)],
            false,
        )
        .unwrap_or_else(|error| panic!("storage exit must build: {error:?}"));

        let matching = CheckedAsync::try_new(
            unit.unit(),
            kind,
            [],
            [],
            [],
            [AsyncScopeExitPlan::new(scope, exit, [], [], [], [], false)],
            false,
        )
        .unwrap_or_else(|error| panic!("matching async exit must build: {error:?}"));

        let mismatched = CheckedAsync::try_new(
            unit.unit(),
            kind,
            [],
            [],
            [],
            [AsyncScopeExitPlan::new(
                scope,
                other_exit,
                [],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("mismatched async exit must build: {error:?}"));

        let storage = StoragePlanBuilder::new(unit.unit(), kind).finish();
        let types = CheckedExpressionTypes::new(unit.unit(), kind, []);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must build: {error:?}"));

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            unit.tree()
                .expressions()
                .map(|(expression, _)| (expression, BoundDependencyContract::new([]))),
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty dependencies must build: {error:?}"));

        let target = test_mir_target();

        assert!(
            VerifiedLoweringPlans::try_new(
                &unit,
                &storage,
                &storage_flow,
                &dependencies,
                &selections,
                available_compiler_known_symbols(),
                &matching,
                target.runtime_abi(),
            )
            .is_ok()
        );

        let Err(error) = VerifiedLoweringPlans::try_new(
            &unit,
            &storage,
            &storage_flow,
            &dependencies,
            &selections,
            available_compiler_known_symbols(),
            &mismatched,
            target.runtime_abi(),
        ) else {
            panic!("mismatched scope exit must fail plan verification");
        };

        assert_eq!(error.kind(), LoweringPlanKind::ScopeExit);
        assert_eq!(error.cause(), LoweringPlanFailureCause::Unexpected);
    }

    #[test]
    fn input_rejects_bound_expressions_without_final_types() {
        let unit = test_runtime_default_unit(6, push_unit_expression);

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let analysis = empty_expression_inputs(&unit);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        assert_input_error(
            lowering_input(&unit, &control_flow, (&analysis).into()),
            LoweringInputError::MissingExpressionType(expression),
        );
    }

    #[test]
    fn input_rejects_valid_expressions_without_required_semantic_selections() {
        let unit = test_runtime_default_unit(6, |tree, origin| {
            let unit_expression = push_unit_expression(tree, origin);

            match tree.push_expression(BoundExpression::Conversion(BoundConversionExpression::new(
                origin,
                unit_expression,
                origin.source_anchor().syntax(),
                None,
                None,
                false,
            ))) {
                Ok(expression) => expression,
                Err(error) => panic!("test conversion must fit: {error:?}"),
            }
        });

        let BoundUnitRoot::Expression(conversion) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let Some(BoundExpression::Conversion(source)) = unit.view().expression(conversion) else {
            panic!("test root must be a conversion");
        };

        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [
                ExpressionTypeEntry::new(source.operand(), result),
                ExpressionTypeEntry::new(conversion, result),
            ],
        );

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("literal-free values must validate: {error:?}"));

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let (storage, storage_flow) = empty_storage_analysis(&unit);

        let patterns = CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], []);
        let ancillary = empty_expression_inputs(&unit);

        let analysis = ExpressionInputReferences {
            types: &types,
            patterns: &patterns,
            selections: &selections,
            literals: &literals,
            storage: &storage,
            storage_flow: &storage_flow,
            values: &values,
            ..(&ancillary).into()
        };

        assert_input_error(
            lowering_input(&unit, &control_flow, analysis),
            LoweringInputError::MissingSemanticSelection(conversion),
        );
    }

    #[test]
    fn input_rejects_storage_flow_that_disagrees_with_the_storage_plan() {
        let (unit, expression, reached_type) = storage_expression_unit(7);

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test identity must validate: {error:?}"));

        let access = push_storage_access(
            &mut storage,
            identity,
            reached_type,
            bound.origin().source_anchor(),
        );

        storage
            .plan_access(expression, StorageAccessPurpose::Read, access)
            .unwrap_or_else(|error| panic!("test access plan must validate: {error:?}"));

        let storage = storage.finish();

        let incomplete_flow =
            StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
                .unwrap_or_else(|error| panic!("incomplete test flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_analysis(&unit, &storage, &incomplete_flow),
            Err(LoweringInputError::StorageOperationCountMismatch {
                expected: 1,
                actual: 0,
            })
        );

        let flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [StorageOperationDecision::new(
                expression,
                StorageAccessPurpose::Write,
                access,
                None,
                StorageOperationStatus::Valid,
            )],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_analysis(&unit, &storage, &flow),
            Err(LoweringInputError::InvalidStorageOperation(expression))
        );
    }

    #[test]
    fn input_rejects_a_borrow_decision_with_the_wrong_capability() {
        let (unit, expression, reached_type) = storage_expression_unit(8);

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test identity must validate: {error:?}"));

        let access = push_storage_access(
            &mut storage,
            identity,
            reached_type,
            bound.origin().source_anchor(),
        );

        let wrong_capability = storage
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Expression(expression),
                BorrowKind::Mutable,
                access,
                None,
                bound.origin().source_anchor(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test borrow capability must validate: {error:?}"));

        storage
            .plan_access(
                expression,
                StorageAccessPurpose::Borrow(BorrowKind::Shared),
                access,
            )
            .unwrap_or_else(|error| panic!("test access plan must validate: {error:?}"));

        let storage = storage.finish();

        let flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [StorageOperationDecision::new(
                expression,
                StorageAccessPurpose::Borrow(BorrowKind::Shared),
                access,
                Some(wrong_capability),
                StorageOperationStatus::Valid,
            )],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_analysis(&unit, &storage, &flow),
            Err(LoweringInputError::InvalidStorageOperation(expression))
        );
    }

    fn push_unit_expression(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> BoundExpressionId {
        tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Unit,
            [],
            [],
            [],
            None,
            false,
        )))
        .unwrap_or_else(|error| panic!("test unit expression must fit: {error:?}"))
    }

    fn push_storage_access(
        storage: &mut StoragePlanBuilder,
        identity: StorageIdentityId,
        reached_type: TypeId,
        source: BoundSourceAnchor,
    ) -> StorageAccessId {
        storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                reached_type,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test access must validate: {error:?}"))
    }

    fn storage_expression_unit(id: u32) -> (BoundUnit, BoundExpressionId, TypeId) {
        let unit = test_runtime_default_unit(id, push_unit_expression);

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let reached_type = semantic_values()
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        (unit, expression, reached_type)
    }

    #[test]
    fn input_is_safe_to_share_between_lowering_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LoweringInput<'static>>();
    }

    fn assert_input_error(
        result: Result<LoweringInput<'_>, LoweringInputError>,
        expected: LoweringInputError,
    ) {
        let error = match result {
            Ok(_) => panic!("invalid lowering input must be rejected"),
            Err(error) => error,
        };

        assert_eq!(error, expected);
    }

    struct ExpressionInputs {
        types: CheckedExpressionTypes,
        patterns: CheckedPatterns,
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
        storage: bray_bound_tree::StoragePlan,
        liveness: Liveness,
        refinements: CheckedRefinements,
        storage_flow: StorageFlow,
        dependencies: CheckedDependencyContracts,
        async_analysis: CheckedAsync,
        behavior: CheckedBodyBehavior,
        values: SemanticValueStore,
    }

    #[derive(Clone, Copy)]
    struct ExpressionInputReferences<'inputs> {
        types: &'inputs CheckedExpressionTypes,
        patterns: &'inputs CheckedPatterns,
        selections: &'inputs CheckedSemanticSelections,
        literals: &'inputs CheckedLiteralValues,
        storage: &'inputs bray_bound_tree::StoragePlan,
        liveness: &'inputs Liveness,
        refinements: &'inputs CheckedRefinements,
        storage_flow: &'inputs StorageFlow,
        dependencies: &'inputs CheckedDependencyContracts,
        async_analysis: &'inputs CheckedAsync,
        behavior: &'inputs CheckedBodyBehavior,
        values: &'inputs SemanticValueStore,
    }

    impl<'inputs> From<&'inputs ExpressionInputs> for ExpressionInputReferences<'inputs> {
        fn from(analysis: &'inputs ExpressionInputs) -> Self {
            Self {
                types: &analysis.types,
                patterns: &analysis.patterns,
                selections: &analysis.selections,
                literals: &analysis.literals,
                storage: &analysis.storage,
                liveness: &analysis.liveness,
                refinements: &analysis.refinements,
                storage_flow: &analysis.storage_flow,
                dependencies: &analysis.dependencies,
                async_analysis: &analysis.async_analysis,
                behavior: &analysis.behavior,
                values: &analysis.values,
            }
        }
    }

    fn lowering_input<'inputs>(
        unit: &'inputs BoundUnit,
        control_flow: &'inputs CheckedControlFlow,
        analysis: ExpressionInputReferences<'inputs>,
    ) -> Result<LoweringInput<'inputs>, LoweringInputError> {
        let target = test_mir_target();

        let lowering_plans = VerifiedLoweringPlans::try_new(
            unit,
            analysis.storage,
            analysis.storage_flow,
            analysis.dependencies,
            analysis.selections,
            available_compiler_known_symbols(),
            analysis.async_analysis,
            target.runtime_abi(),
        )?;

        LoweringInput::try_new(
            unit,
            control_flow,
            analysis.types,
            analysis.patterns,
            analysis.literals,
            analysis.liveness,
            analysis.refinements,
            lowering_plans,
            analysis.behavior,
            analysis.values,
            bray_ir::MirUnitKind::Synchronous,
            target,
        )
    }

    fn empty_expression_inputs(unit: &BoundUnit) -> ExpressionInputs {
        empty_expression_inputs_with_width(unit, test_mir_target().machine().pointer_width_bits())
    }

    fn empty_expression_inputs_with_width(
        unit: &BoundUnit,
        target_integer_width_bits: NonZeroU16,
    ) -> ExpressionInputs {
        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);
        let patterns = CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], []);

        let selections = CheckedSemanticSelections::try_new(unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let values = semantic_values();

        let literals =
            CheckedLiteralValues::try_new(unit, &types, &values, target_integer_width_bits, [])
                .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let (storage, storage_flow) = empty_storage_analysis(unit);

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("empty liveness analysis must validate: {error:?}"));

        let refinements = CheckedRefinements::try_new(unit.unit(), unit.key().kind(), [], false)
            .unwrap_or_else(|error| panic!("empty refinement analysis must validate: {error:?}"));

        let expressions = unit
            .tree()
            .expressions()
            .map(|(expression, _)| (expression, BoundDependencyContract::new([])));

        let accesses = storage
            .access_entries()
            .map(|(access, _)| (access, BoundDependencyContract::new([])));

        let borrows = storage
            .borrow_capability_entries()
            .map(|(borrow, _)| (borrow, BoundDependencyContract::new([])));

        let dependencies = CheckedDependencyContracts::try_new(
            unit,
            &storage,
            expressions,
            [],
            accesses,
            borrows,
            false,
        )
        .unwrap_or_else(|error| panic!("empty dependency contracts must validate: {error:?}"));

        let async_analysis =
            CheckedAsync::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
                .unwrap_or_else(|error| panic!("empty async analysis must validate: {error:?}"));

        let behavior = CheckedBodyBehavior::new(
            unit.unit(),
            unit.key().kind(),
            CurrentRunCancellation::NotEntered,
            false,
        );

        ExpressionInputs {
            types,
            patterns,
            selections,
            literals,
            storage,
            liveness,
            refinements,
            storage_flow,
            dependencies,
            async_analysis,
            behavior,
            values,
        }
    }

    fn empty_storage_analysis(unit: &BoundUnit) -> (bray_bound_tree::StoragePlan, StorageFlow) {
        let storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind()).finish();

        let storage_flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
            .unwrap_or_else(|error| panic!("empty storage flow must validate: {error:?}"));

        (storage, storage_flow)
    }

    fn semantic_values() -> SemanticValueStore {
        SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must initialize: {error:?}"))
    }
}
