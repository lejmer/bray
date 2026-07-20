use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult};
use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundExpression, BoundReferenceTarget, BoundUnit,
    BoundUnitRoot, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, DeclaredValueTypeConstraint,
    DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence, DeclaredValueTypeTemplates,
    DeclaredValueTypeTerm, walk_bound_unit_view,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, TypeData, TypeExpressionTemplate};
use bray_syntax::TypeExpressionSyntax;

use super::CompilationBinderFacts;
use super::symbol::type_binder;

pub(in crate::compilation) fn bind_declared_value_type_templates(
    context: &CompilationBinderFacts<'_>,
    unit: &BoundUnit,
) -> BinderFactResult<DiagnosticResult<DeclaredValueTypeTemplates>> {
    let owner = context
        .symbols()
        .symbol_for_key(unit.key().declared_owner())
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let mut binding = DeclaredValueTypeBinding::new(context, unit, owner);

    binding.bind_visible_generic_const_parameters()?;
    binding.bind_unit_surface()?;
    binding.bind_bound_tree()?;

    Ok(binding.finish())
}

pub(super) struct DeclaredValueTypeBinding<'facts> {
    pub(super) context: &'facts CompilationBinderFacts<'facts>,
    pub(super) unit: &'facts BoundUnit,
    pub(super) owner: AnySymbolId,
    pub(super) evidence: Vec<DeclaredValueTypeEvidence>,
    pub(super) constraints: Vec<DeclaredValueTypeConstraint>,
    pub(super) callable_result: Option<TypeExpressionTemplate>,
    pub(super) diagnostics: DiagnosticBag,
}

impl<'facts> DeclaredValueTypeBinding<'facts> {
    fn new(
        context: &'facts CompilationBinderFacts<'facts>,
        unit: &'facts BoundUnit,
        owner: AnySymbolId,
    ) -> Self {
        Self {
            context,
            unit,
            owner,
            evidence: Vec::new(),
            constraints: Vec::new(),
            callable_result: None,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn bind_bound_tree(&mut self) -> BinderFactResult<()> {
        let root = root_node(self.unit.root());

        let mut cancelled = false;
        let mut failure = None;

        let outcome = walk_bound_unit_view(self.unit.view(), root, |event| {
            if self.context.is_cancelled() {
                cancelled = true;

                return BoundWalkControl::Stop;
            }

            if let BoundWalkEvent::Enter(node) = event
                && let Err(error) = self.bind_node(node)
            {
                failure = Some(error);

                return BoundWalkControl::Stop;
            }

            BoundWalkControl::Continue
        });

        if cancelled {
            return Err(BinderFactError::Cancelled);
        }

        if let Some(error) = failure {
            return Err(error);
        }

        match outcome {
            BoundWalkOutcome::Completed => Ok(()),
            BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
                Err(BinderFactError::DependencyUnavailable)
            }
        }
    }

    fn bind_node(&mut self, node: AnyBoundNodeId) -> BinderFactResult<()> {
        match node {
            AnyBoundNodeId::Expression(id) => self.bind_expression(id),
            AnyBoundNodeId::Pattern(id) => {
                let pattern = self
                    .unit
                    .tree()
                    .pattern(id)
                    .ok_or(BinderFactError::DependencyUnavailable)?;

                for binding in pattern.bindings() {
                    self.add_constraint(
                        DeclaredValueTypeConstraintKind::PatternBinding,
                        DeclaredValueTypeTerm::Pattern(id),
                        local_value((*binding).into()),
                    );
                }

                Ok(())
            }
            AnyBoundNodeId::Block(id) => self.bind_block(id),
            AnyBoundNodeId::CallableBody(_) => Ok(()),
        }
    }

    fn bind_expression(&mut self, id: bray_bound_tree::BoundExpressionId) -> BinderFactResult<()> {
        let expression = self
            .unit
            .tree()
            .expression(id)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        if let BoundExpression::Name(name) = expression {
            self.add_constraint(
                DeclaredValueTypeConstraintKind::DefinitionUse,
                DeclaredValueTypeTerm::Expression(id),
                DeclaredValueTypeTerm::Value(name.target()),
            );
        }

        Ok(())
    }

    fn bind_block(&mut self, id: bray_bound_tree::BoundBlockId) -> BinderFactResult<()> {
        let block = self
            .unit
            .tree()
            .block(id)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    let pattern = DeclaredValueTypeTerm::Pattern(binding.pattern());

                    self.add_constraint(
                        DeclaredValueTypeConstraintKind::Initializer,
                        DeclaredValueTypeTerm::Expression(binding.initializer()),
                        pattern,
                    );

                    if let Some(declared) = binding.declared_type() {
                        let template = self.bind_type_anchor(declared.syntax())?;

                        self.add_evidence(pattern, template);
                    }
                }
                BoundBlockItem::LocalConstant(constant) => {
                    let template = self.bind_type_anchor(constant.declared_type().syntax())?;
                    let initializer = DeclaredValueTypeTerm::Expression(constant.initializer());

                    match constant.symbol() {
                        Some(symbol) => {
                            let value = local_value(symbol.into());

                            self.add_evidence(value, template);

                            self.add_constraint(
                                DeclaredValueTypeConstraintKind::Initializer,
                                initializer,
                                value,
                            );
                        }
                        None => self.add_evidence(initializer, template),
                    }
                }
                BoundBlockItem::Expression(_) => {}
            }
        }

        Ok(())
    }

    fn bind_type_anchor(
        &mut self,
        anchor: bray_declarations::SyntaxAnchor,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        let Some(syntax) = anchor.find_descendant::<TypeExpressionSyntax>(self.context.syntax())
        else {
            return self.error_type_template();
        };

        self.bind_type_syntax(&syntax)
    }

    pub(super) fn bind_type_syntax(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BinderFactResult<TypeExpressionTemplate> {
        let result = type_binder(self.context, self.owner)?.bind_type_expression(syntax)?;

        let (template, diagnostics) = result.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(template)
    }

    fn error_type_template(&self) -> BinderFactResult<TypeExpressionTemplate> {
        self.context
            .semantic_values()
            .intern_type(TypeData::Error)
            .map(TypeExpressionTemplate::Resolved)
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }

    pub(super) fn add_evidence(
        &mut self,
        term: DeclaredValueTypeTerm,
        template: TypeExpressionTemplate,
    ) {
        self.evidence
            .push(DeclaredValueTypeEvidence::new(term, template));
    }

    pub(super) fn add_constraint(
        &mut self,
        kind: DeclaredValueTypeConstraintKind,
        left: DeclaredValueTypeTerm,
        right: DeclaredValueTypeTerm,
    ) {
        self.constraints
            .push(DeclaredValueTypeConstraint::new(kind, left, right));
    }

    pub(super) fn check_cancellation(&self) -> BinderFactResult<()> {
        if self.context.is_cancelled() {
            Err(BinderFactError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn finish(self) -> DiagnosticResult<DeclaredValueTypeTemplates> {
        DiagnosticResult::new(
            DeclaredValueTypeTemplates::new(
                self.unit.unit(),
                self.unit.key().kind(),
                self.evidence,
                self.constraints,
                self.callable_result,
            ),
            self.diagnostics,
        )
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

pub(super) const fn local_value(symbol: AnyLocalSymbolId) -> DeclaredValueTypeTerm {
    DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(symbol))
}
