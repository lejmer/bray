use crate::compilation::binder::CompilationBindingContext;
use crate::fact::FactQueryError;
use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpressionId, BoundReferenceTarget, BoundUnit, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedSemanticSelections, CheckedTemplate, CheckedTemplateBehavior,
    CheckedTemplateInput, CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind,
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::TypeId;
use std::collections::BTreeMap;

pub(in crate::compilation) struct SourceTemplateInput {
    pub(in crate::compilation::template) kind: CheckedTemplateInputKind,
    pub(in crate::compilation) target: Option<BoundReferenceTarget>,
    pub(in crate::compilation) ty: TypeId,
}

impl SourceTemplateInput {
    pub(in crate::compilation) const fn new(
        kind: CheckedTemplateInputKind,
        target: Option<BoundReferenceTarget>,
        ty: TypeId,
    ) -> Self {
        Self { kind, target, ty }
    }
}

pub(in crate::compilation) struct SourceTemplateRequest<'a> {
    pub(in crate::compilation) kind: CheckedTemplateKind,
    pub(in crate::compilation) unit: &'a BoundUnit,
    pub(in crate::compilation) patterns: &'a bray_bound_tree::CheckedPatterns,
    pub(in crate::compilation) types: &'a CheckedExpressionTypes,
    pub(in crate::compilation) selections: &'a CheckedSemanticSelections,
    pub(in crate::compilation) literals: &'a CheckedLiteralValues,
    pub(in crate::compilation) storage: &'a bray_bound_tree::StoragePlan,
    pub(in crate::compilation) flow: &'a bray_bound_tree::StorageFlow,
    pub(in crate::compilation) expression: SyntaxAnchor,
    pub(in crate::compilation) inputs: Vec<SourceTemplateInput>,
    pub(in crate::compilation) execution: Option<&'a bray_checker::ExecutionCandidate>,
    pub(in crate::compilation) behavior: CheckedTemplateBehavior,
}

pub(in crate::compilation) fn build_source_template(
    context: &CompilationBindingContext<'_>,
    request: SourceTemplateRequest,
) -> Result<CheckedTemplate, FactQueryError> {
    let checker_context = context
        .compilation()
        .checker_context_for(request.unit.key(), context.cancellation())?;

    let semantic_context =
        bray_binder::semantic_unit_context(checker_context.symbols(), request.unit);

    let checker =
        bray_checker::CheckerUnitView::new(request.unit, &semantic_context, &checker_context);

    let mut builder = SourceTemplateBuilder::new(context, &request, checker)?;

    let result = if let bray_bound_tree::BoundUnitRoot::CallableBody { body, .. }
    | bray_bound_tree::BoundUnitRoot::AnonymousCallable { body, .. } =
        request.unit.root()
    {
        let body = request
            .unit
            .view()
            .callable_body(body)
            .expect("checked callable retains its body");

        let bray_bound_tree::BoundCallableBodyKind::Block(block) = body.kind() else {
            panic!("checked const-callable body is not recovered");
        };

        let result = request
            .types
            .callable_result_type()
            .expect("checked callable retains its result type");

        builder.callable_body(block, result)?
    } else {
        let root = request
            .unit
            .declaration_expression(request.expression)
            .unwrap_or_else(|| {
                panic!(
                    "checked source template {:?} retains expression {:?}",
                    request.kind, request.expression
                )
            });

        let result_type = request
            .types
            .expression(root)
            .expect("checked template root retains its type")
            .ty();

        builder.return_type = Some(result_type);

        let result = builder.expression(root)?;

        builder.finish_exit(
            bray_bound_tree::SelectedPropagationBoundary::Callable,
            result,
            result_type,
        )?
    };

    assert!(
        builder.exits.is_empty(),
        "checked template resolves every selected value boundary"
    );

    for (target, id) in &builder.input_ids {
        let input =
            &mut builder.inputs[usize::try_from(id.raw()).expect("checked input fits host")];

        if input.state_output().is_some() {
            let binding = bray_bound_tree::StorageBindingTarget::from_reference_target(*target)
                .and_then(|target| builder.storage.binding(target))
                .expect("mutable formal retains its storage binding");

            let root = match binding {
                bray_bound_tree::StorageBinding::Identity(root) => root,
                bray_bound_tree::StorageBinding::Access(access) => builder
                    .storage
                    .root_identity(access)
                    .expect("mutable formal frame retains its entry storage"),
            };

            *input = input
                .clone()
                .with_state_output(Some(builder.state_values[&root]));
        }
    }

    Ok(CheckedTemplate::from_checked_parts(
        request.kind,
        builder.inputs,
        builder.nodes,
        builder.temporaries,
        result,
        request.behavior,
    ))
}

pub(in crate::compilation) struct SourceTemplateBuilder<'context, 'compilation, 'unit> {
    pub(in crate::compilation::template) context: &'context CompilationBindingContext<'compilation>,
    pub(in crate::compilation::template) unit: &'unit BoundUnit,
    pub(in crate::compilation::template) kind: CheckedTemplateKind,
    pub(in crate::compilation::template) checker: bray_checker::CheckerUnitView<
        'context,
        crate::compilation::checker::CompilationCheckerContext<'compilation>,
    >,
    pub(in crate::compilation::template) patterns: &'unit bray_bound_tree::CheckedPatterns,
    pub(in crate::compilation::template) types: &'unit CheckedExpressionTypes,
    pub(in crate::compilation) selections: &'unit CheckedSemanticSelections,
    pub(super) literals: &'unit CheckedLiteralValues,
    pub(in crate::compilation::template) storage: &'unit bray_bound_tree::StoragePlan,
    pub(in crate::compilation::template) flow: &'unit bray_bound_tree::StorageFlow,
    pub(in crate::compilation::template) execution: Option<&'unit bray_checker::ExecutionCandidate>,
    pub(in crate::compilation::template) inputs: Vec<CheckedTemplateInput>,
    pub(in crate::compilation::template) input_ids:
        BTreeMap<BoundReferenceTarget, CheckedTemplateInputId>,
    pub(in crate::compilation::template) nodes: Vec<CheckedTemplateNode>,
    pub(in crate::compilation::template) temporaries:
        Vec<bray_bound_tree::CheckedTemplateTemporary>,
    pub(super) expression_nodes:
        BTreeMap<BoundExpressionId, (CheckedTemplateNodeId, Option<CheckedTemplateNodeId>)>,
    pub(in crate::compilation::template) bindings:
        BTreeMap<BoundReferenceTarget, CheckedTemplateNodeId>,
    pub(in crate::compilation::template) state_values:
        BTreeMap<bray_bound_tree::StorageIdentityId, CheckedTemplateNodeId>,
    pub(in crate::compilation::template) state_changes: Vec<super::super::state::StateChange>,
    pub(in crate::compilation::template) loops: Vec<super::super::iteration::LoopScope>,
    pub(in crate::compilation::template) path: super::super::control::TemplatePath,
    pub(in crate::compilation::template) evaluation: Option<CheckedTemplateNodeId>,
    pub(in crate::compilation::template) exits: BTreeMap<
        bray_bound_tree::SelectedPropagationBoundary,
        super::super::control::TemplateBoundary,
    >,
    pub(in crate::compilation::template) return_type: Option<TypeId>,
    pub(in crate::compilation::template) generator_observations:
        bray_bound_tree::GeneratorObservations,
    pub(in crate::compilation::template) generator_values:
        BTreeMap<BoundExpressionId, bray_bound_tree::StorageIdentityId>,
    pub(in crate::compilation::template) generator_targets:
        BTreeMap<SyntaxAnchor, bray_bound_tree::StorageIdentityId>,
    pub(in crate::compilation::template) pattern_values:
        BTreeMap<bray_bound_tree::BoundPatternId, CheckedTemplateNodeId>,
}

impl<'context, 'compilation, 'unit> SourceTemplateBuilder<'context, 'compilation, 'unit> {
    pub(super) fn new(
        context: &'context CompilationBindingContext<'compilation>,
        request: &SourceTemplateRequest<'unit>,
        checker: bray_checker::CheckerUnitView<
            'context,
            crate::compilation::checker::CompilationCheckerContext<'compilation>,
        >,
    ) -> Result<Self, FactQueryError> {
        let mut inputs = Vec::with_capacity(request.inputs.len());
        let mut input_ids = BTreeMap::new();
        let mut nodes = Vec::new();
        let bindings = BTreeMap::new();
        let mut state_values = BTreeMap::new();

        for input in &request.inputs {
            let id = CheckedTemplateInputId::new(index(inputs.len()));

            if let Some(target) = input.target
                && input_ids.insert(target, id).is_some()
            {
                return Err(incomplete("duplicate_template_input_target"));
            }

            let state_type = (request.kind == CheckedTemplateKind::ConstantCallableBody)
                .then(|| context.semantic_values().type_data(input.ty))
                .and_then(|ty| match ty.as_ref() {
                    bray_symbols::TypeData::Borrow {
                        kind: bray_symbols::BorrowKind::Mutable,
                        target,
                    } => Some(*target),
                    _ => None,
                });

            let ty = state_type.unwrap_or(input.ty);
            let mut checked = CheckedTemplateInput::new(input.kind.clone(), ty);

            let root = input
                .target
                .and_then(bray_bound_tree::StorageBindingTarget::from_reference_target)
                .and_then(|target| request.storage.binding(target))
                .and_then(|binding| match binding {
                    bray_bound_tree::StorageBinding::Identity(root) => Some(root),
                    bray_bound_tree::StorageBinding::Access(access) => {
                        request.storage.root_identity(access)
                    }
                });

            if let Some(root) = root {
                let value = CheckedTemplateNodeId::new(index(nodes.len()));

                nodes.push(CheckedTemplateNode::new(
                    CheckedTemplateOperation::Input(id),
                    ty,
                ));

                state_values.insert(root, value);

                if state_type.is_some() {
                    checked = checked.with_state_output(Some(value));
                }
            }

            inputs.push(checked);
        }

        let prepared = context
            .compilation()
            .prepared_body_analysis_with_cancellation(
                request.unit.key().clone(),
                context.cancellation(),
            )?;

        Ok(Self {
            // Retain the cached index independently of this local prepared-fact handle.
            generator_observations: prepared.result().value().generator_observations().clone(),
            generator_values: BTreeMap::new(),
            generator_targets: BTreeMap::new(),
            context,
            unit: request.unit,
            kind: request.kind,
            checker,
            patterns: request.patterns,
            types: request.types,
            selections: request.selections,
            literals: request.literals,
            storage: request.storage,
            flow: request.flow,
            execution: request.execution,
            inputs,
            input_ids,
            nodes,
            temporaries: Vec::new(),
            expression_nodes: BTreeMap::new(),
            bindings,
            state_values,
            state_changes: Vec::new(),
            loops: Vec::new(),
            path: super::super::control::TemplatePath::Always,
            evaluation: None,
            exits: [(
                bray_bound_tree::SelectedPropagationBoundary::Callable,
                super::super::control::TemplateBoundary::new(0),
            )]
            .into(),
            return_type: None,
            pattern_values: BTreeMap::new(),
        })
    }
}

pub(in crate::compilation::template) fn index(length: usize) -> u32 {
    u32::try_from(length).expect("checked source template identity fits its bound unit")
}

pub(in crate::compilation::template) fn incomplete(reason: &'static str) -> FactQueryError {
    panic!("checked source template construction violated its phase contract: {reason}")
}
