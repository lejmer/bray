use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    GenericOwnerId, GenericTypeConstraintsQuery, SymbolQueryRequest, TypeExpressionTemplate,
    TypeId,
};

use crate::compilation::SemanticQueryContext;
use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::{CompilationFactKey, FactQueryError};

pub(super) fn type_template_context(template: &TypeExpressionTemplate) -> SemanticQueryContext {
    if let Some(occurrence) = template.constant_expressions().first() {
        return SemanticQueryContext::Symbol(occurrence.key().owner());
    }

    match template {
        TypeExpressionTemplate::Named { definition, .. } => {
            SemanticQueryContext::Symbol(match definition {
                bray_symbols::NamedTypeSymbolId::Struct(definition) => (*definition).into(),
                bray_symbols::NamedTypeSymbolId::Union(definition) => (*definition).into(),
            })
        }
        TypeExpressionTemplate::CallableContract { definition, .. } => {
            SemanticQueryContext::Symbol((*definition).into())
        }
        TypeExpressionTemplate::TypeValuedMemberProjection { member, .. } => {
            SemanticQueryContext::Symbol((*member).into())
        }
        TypeExpressionTemplate::TraitView(application) => {
            SemanticQueryContext::Symbol(application.definition().into())
        }
        TypeExpressionTemplate::Resolved(_)
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::FlexibleArray(_)
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::Callable(_) => {
            SemanticQueryContext::Fact(CompilationFactKey::CheckDiagnostics)
        }
    }
}

pub(in crate::compilation) fn lexical_type_equalities(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
) -> Result<DiagnosticResult<Vec<(TypeId, TypeId)>>, FactQueryError> {
    let mut equalities = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    if key.kind() == BoundUnitKind::TargetGate {
        return Ok(DiagnosticResult::new(equalities, diagnostics));
    }

    let mut owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .expect("bound checking context retains its declaration");

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let template = context
                .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                    generic_owner,
                ))
                .map_err(binding_query_error)?;

            diagnostics.extend(template.diagnostics().iter().cloned());

            let has_equalities =
                template
                    .value()
                    .constraints()
                    .iter()
                    .any(|constraint| match constraint {
                        GenericConstraintTemplate::TypeEquality { .. } => true,
                        GenericConstraintTemplate::Resolved(constraint) => {
                            constraint.type_equality_requirement().is_some()
                        }
                        _ => false,
                    });

            if has_equalities
                && constraint_scope(template.value(), key)
                    != bray_checker::GenericConstraintScope::Unavailable
            {
                let constraints = context
                    .resolve_symbol_query(SymbolQueryRequest::<GenericTypeConstraintsQuery>::new(
                        generic_owner,
                    ))
                    .map_err(binding_query_error)?;

                diagnostics.extend(constraints.diagnostics().iter().cloned());

                if !constraints.diagnostics().has_errors() {
                    equalities.extend(
                        constraints
                            .value()
                            .constraints()
                            .iter()
                            .filter_map(|constraint| constraint.type_equality_requirement()),
                    );
                }
            }
        }

        let Some(container) = context.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    Ok(DiagnosticResult::new(equalities, diagnostics))
}

fn constraint_scope(
    template: &GenericDeclarationTemplate,
    key: &BoundUnitKey,
) -> bray_checker::GenericConstraintScope {
    let source = key.source().syntax();

    let contains = |occurrences: Vec<bray_symbols::ConstantExpressionOccurrence>| {
        occurrences.into_iter().any(|occurrence| {
            let syntax = occurrence.key().syntax();

            syntax.source_id() == source.source_id()
                && syntax.full_range().contains_range(source.full_range())
        })
    };

    // A header's embedded constants and their nested units precede that header's premises.
    let checking_header = template
        .constraints()
        .iter()
        .any(|constraint| match constraint {
            GenericConstraintTemplate::TraitSatisfaction {
                subject,
                application,
                ..
            } => {
                contains(subject.constant_expressions())
                    || contains(application.constant_expressions())
            }
            GenericConstraintTemplate::TypeEquality { left, right, .. } => {
                contains(left.constant_expressions()) || contains(right.constant_expressions())
            }
            GenericConstraintTemplate::Source { .. } | GenericConstraintTemplate::Resolved(_) => {
                false
            }
        });

    if checking_header || key.kind() == BoundUnitKind::TargetGate {
        return bray_checker::GenericConstraintScope::Unavailable;
    }

    let checking_predicate = template.constraints().iter().any(|constraint| {
        constraint.expression().is_some_and(|expression| {
            let syntax = expression.syntax();

            syntax.source_id() == source.source_id()
                && syntax.full_range().contains_range(source.full_range())
        }) || (key.kind() == BoundUnitKind::Constraint && constraint.unit_syntax() == Some(source))
    });

    if checking_predicate {
        bray_checker::GenericConstraintScope::TypeHeader
    } else {
        bray_checker::GenericConstraintScope::Complete
    }
}

pub(in crate::compilation) fn generic_constraint_scope(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
    owner: GenericOwnerId,
) -> Result<DiagnosticResult<bray_checker::GenericConstraintScope>, FactQueryError> {
    let template = context
        .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
            owner,
        ))
        .map_err(binding_query_error)?;

    // The scope result retains the immutable diagnostics of its declaration template.
    Ok(DiagnosticResult::new(
        constraint_scope(template.value(), key),
        template.diagnostics().clone(),
    ))
}
