use crate::compilation::binder::BindingQueryResult;
use bray_binder::{BindingQueryContext, BindingQueryError};
use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundExpression, BoundReferenceTarget, BoundUnit,
    BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, DeclaredValueTypeConstraint,
    DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence, DeclaredValueTypeTemplates,
    DeclaredValueTypeTerm, walk_bound_unit_view,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, TypeData, TypeExpressionTemplate};
use bray_syntax::TypeExpressionSyntax;

use super::CompilationBindingContext;
use super::symbol::type_binder;

pub(in crate::compilation) fn bind_declared_value_type_templates(
    context: &CompilationBindingContext<'_>,
    unit: &BoundUnit,
) -> BindingQueryResult<DiagnosticResult<DeclaredValueTypeTemplates>> {
    let owner = context
        .symbols()
        .symbol_for_key(unit.key().declared_owner())
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let mut binding = DeclaredValueTypeBinding::new(context, unit, owner);

    binding.bind_visible_generic_const_parameters()?;
    binding.bind_unit_surface()?;
    binding.bind_bound_tree()?;

    Ok(binding.finish())
}

pub(super) struct DeclaredValueTypeBinding<'binding> {
    pub(super) context: &'binding CompilationBindingContext<'binding>,
    pub(super) unit: &'binding BoundUnit,
    pub(super) owner: AnySymbolId,
    pub(super) evidence: Vec<DeclaredValueTypeEvidence>,
    pub(super) constraints: Vec<DeclaredValueTypeConstraint>,
    pub(super) callable_type: Option<TypeExpressionTemplate>,
    pub(super) callable_result: Option<TypeExpressionTemplate>,
    pub(super) diagnostics: DiagnosticBag,
}

impl<'binding> DeclaredValueTypeBinding<'binding> {
    fn new(
        context: &'binding CompilationBindingContext<'binding>,
        unit: &'binding BoundUnit,
        owner: AnySymbolId,
    ) -> Self {
        Self {
            context,
            unit,
            owner,
            evidence: Vec::new(),
            constraints: Vec::new(),
            callable_type: None,
            callable_result: None,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn bind_bound_tree(&mut self) -> BindingQueryResult<()> {
        let root = AnyBoundNodeId::from(self.unit.root());

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
            return Err(BindingQueryError::Cancelled);
        }

        if let Some(error) = failure {
            return Err(error);
        }

        match outcome {
            BoundWalkOutcome::Completed => Ok(()),
            BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
                Err(BindingQueryError::DependencyUnavailable)
            }
        }
    }

    fn bind_node(&mut self, node: AnyBoundNodeId) -> BindingQueryResult<()> {
        match node {
            AnyBoundNodeId::Expression(id) => self.bind_expression(id),
            AnyBoundNodeId::Pattern(id) => {
                let pattern = self
                    .unit
                    .tree()
                    .pattern(id)
                    .ok_or(BindingQueryError::DependencyUnavailable)?;

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

    fn bind_expression(
        &mut self,
        id: bray_bound_tree::BoundExpressionId,
    ) -> BindingQueryResult<()> {
        let expression = self
            .unit
            .tree()
            .expression(id)
            .ok_or(BindingQueryError::DependencyUnavailable)?;

        match expression {
            BoundExpression::Name(name) => {
                self.bind_surface_reference_type(name.target())?;

                self.add_constraint(
                    DeclaredValueTypeConstraintKind::DefinitionUse,
                    DeclaredValueTypeTerm::Expression(id),
                    DeclaredValueTypeTerm::Value(name.target()),
                );
            }
            BoundExpression::Conversion(conversion) => {
                let template = self.bind_type_anchor(conversion.target_syntax())?;

                self.add_evidence(DeclaredValueTypeTerm::Expression(id), template);
            }
            _ => {}
        }

        Ok(())
    }

    fn bind_block(&mut self, id: bray_bound_tree::BoundBlockId) -> BindingQueryResult<()> {
        let block = self
            .unit
            .tree()
            .block(id)
            .ok_or(BindingQueryError::DependencyUnavailable)?;

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
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let Some(syntax) = anchor.find_descendant::<TypeExpressionSyntax>(self.context.syntax())
        else {
            return self.error_type_template();
        };

        self.bind_type_syntax(&syntax)
    }

    pub(super) fn bind_type_syntax(
        &mut self,
        syntax: &TypeExpressionSyntax,
    ) -> BindingQueryResult<TypeExpressionTemplate> {
        let result = type_binder(self.context, self.owner)?.bind_type_expression(syntax)?;

        let (template, diagnostics) = result.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(template)
    }

    fn error_type_template(&self) -> BindingQueryResult<TypeExpressionTemplate> {
        self.context
            .semantic_values()
            .intern_type(TypeData::Error)
            .map(TypeExpressionTemplate::Resolved)
            .map_err(super::semantic_value_binding_error)
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

    pub(super) fn check_cancellation(&self) -> BindingQueryResult<()> {
        if self.context.is_cancelled() {
            Err(BindingQueryError::Cancelled)
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
                self.callable_type,
                self.callable_result,
            ),
            self.diagnostics,
        )
    }
}

pub(super) const fn local_value(symbol: AnyLocalSymbolId) -> DeclaredValueTypeTerm {
    DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(symbol))
}
