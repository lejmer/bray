use bray_diagnostics::DiagnosticFailureField;

use super::diagnostic_context::{
    boolean_field, count_field, identity_field, push_symbol, text_field,
};
use crate::compilation::SemanticQueryContext;

pub(super) fn semantic_query_context(
    context: &SemanticQueryContext,
) -> Vec<DiagnosticFailureField> {
    use SemanticQueryContext as Context;

    let mut fields = Vec::new();

    let kind = match context {
        Context::Fact(fact) => {
            fields.push(identity_field("fact", fact));

            "fact"
        }
        Context::SymbolQuery(query) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", query.symbol());
            fields.push(text_field("query_kind", query.kind().as_str()));

            "symbol_query"
        }
        Context::Symbol(symbol) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);

            "symbol"
        }
        Context::SymbolKey(key) => {
            fields.extend([
                text_field("symbol_key_kind", key.kind().as_str()),
                identity_field("symbol_key", key),
            ]);

            "symbol_key"
        }
        Context::Unit(unit) => {
            push_bound_unit_key(&mut fields, unit);

            "unit"
        }
        Context::Expression { unit, expression } => {
            push_bound_unit_key(&mut fields, unit);
            push_bound_expression(&mut fields, *expression);

            "expression"
        }
        Context::BoundExpression { unit, expression } => {
            fields.push(count_field("bound_unit", u64::from(unit.raw())));
            push_bound_expression(&mut fields, *expression);

            "bound_expression"
        }
        Context::CompilerKnownDeclaration(declaration) => {
            fields.push(text_field(
                "compiler_known_declaration",
                declaration.as_str(),
            ));

            "compiler_known_declaration"
        }
        Context::CompilerKnownDeclarationName(name) => {
            fields.push(text_field("declaration_name", *name));

            "compiler_known_declaration_name"
        }
        Context::CompilerKnownRepresentation(role) => {
            fields.push(text_field("representation_role", role.as_str()));

            "compiler_known_representation"
        }
        Context::Declaration(declaration) => {
            fields.push(identity_field("declaration", declaration));

            "declaration"
        }
        Context::LocalReference {
            unit,
            expression,
            local,
        } => {
            push_bound_unit_key(&mut fields, unit);
            push_bound_expression(&mut fields, *expression);
            push_local_symbol(&mut fields, *local);

            "local_reference"
        }
        Context::Source(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source"
        }
        Context::Type(ty) => {
            fields.push(identity_field("semantic_type", ty));

            "type"
        }
        Context::ImplementationDomain(domain) => {
            fields.push(text_field("coherence_package", domain.package().as_str()));

            "implementation_domain"
        }
    };

    fields.insert(0, text_field("context_kind", kind));

    fields
}

fn push_bound_unit_key(
    fields: &mut Vec<DiagnosticFailureField>,
    unit: &bray_bound_tree::BoundUnitKey,
) {
    let source = unit.source();
    let syntax = source.syntax();

    fields.extend([
        text_field("unit_kind", unit.kind().as_str()),
        identity_field("unit", unit),
        count_field("unit_source", u64::from(syntax.source_id().raw())),
        count_field(
            "unit_source_start",
            u64::from(syntax.full_range().start().bytes()),
        ),
        count_field(
            "unit_source_end",
            u64::from(syntax.full_range().end().bytes()),
        ),
        count_field("unit_source_version", source.source_version().raw()),
        boolean_field("unit_source_recovered", syntax.is_recovered()),
    ]);
}

fn push_bound_expression(
    fields: &mut Vec<DiagnosticFailureField>,
    expression: bray_bound_tree::BoundExpressionId,
) {
    fields.extend([
        count_field("expression_unit", u64::from(expression.unit().raw())),
        count_field("expression", u64::from(expression.ordinal())),
    ]);
}

fn push_local_symbol(
    fields: &mut Vec<DiagnosticFailureField>,
    local: bray_symbols::AnyLocalSymbolId,
) {
    use bray_symbols::AnyLocalSymbolId as Local;

    let ordinal = match local {
        Local::Binding(local) => local.ordinal(),
        Local::Constant(local) => local.ordinal(),
        Local::AnonymousCallable(local) => local.ordinal(),
        Local::AnonymousCallableParameter(local) => local.ordinal(),
        Local::PostconditionResult(local) => local.ordinal(),
    };

    fields.extend([
        text_field("local_kind", local.kind().as_str()),
        count_field("local_region", u64::from(local.region().raw())),
        count_field("local", u64::from(ordinal)),
    ]);
}
