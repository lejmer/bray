use std::sync::Arc;

use bray_binder::{SymbolQueryErrorProvider, SymbolQueryProvider};
use bray_symbols::{
    DependencyContractTemplateId, DependencyRequirement, DependencyRequirementKind,
    DependencySubjectRoot, SymbolGraph, SymbolOrdinal, SymbolOrigin, SymbolQueryContract,
    SymbolQueryRequest, TypeData, TypeExpressionTemplate, TypeId,
};

use super::super::context::CompilationBindingContext;
use crate::compilation::Compilation;
use crate::fact::CancellationToken;

pub(super) fn source_id<T, I: Copy>(
    symbols: &[T],
    origin: impl Fn(&T) -> SymbolOrigin,
    id: impl Fn(&T) -> I,
) -> I {
    let Some(symbol) = symbols
        .iter()
        .find(|symbol| origin(symbol) == SymbolOrigin::Source)
    else {
        panic!("test source must contain the expected declaration");
    };

    id(symbol)
}

pub(super) fn binding_context<'compilation>(
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
) -> CompilationBindingContext<'compilation> {
    match compilation.binding_context(cancellation) {
        Ok(binding_context) => binding_context,
        Err(error) => panic!("source binder context must be available: {error:?}"),
    }
}

pub(super) fn resolved_query<C>(
    binding_context: &CompilationBindingContext<'_>,
    request: SymbolQueryRequest<C>,
) -> Arc<bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>>
where
    C: SymbolQueryContract,
    for<'binding_context> CompilationBindingContext<'binding_context>: SymbolQueryErrorProvider<UpstreamError = crate::fact::FactQueryError>
        + SymbolQueryProvider<C>,
{
    match binding_context.resolve_symbol_query(request) {
        Ok(result) => result,
        Err(error) => panic!("test symbol query must bind: {error:?}"),
    }
}

pub(super) fn symbol_graph(compilation: &Compilation) -> &SymbolGraph {
    match compilation.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test symbol graph must build: {error:?}"),
    }
}

pub(super) fn type_data(compilation: &Compilation, ty: impl ResolvedTestType) -> Arc<TypeData> {
    let values = semantic_values(compilation);
    let ty = ty.resolved_type();

    values.type_data(ty)
}

pub(super) fn assert_parameter_dependency_contract(
    compilation: &Compilation,
    contract: DependencyContractTemplateId,
    ordinal: u32,
    expected: &[DependencyRequirementKind],
) {
    let data = semantic_values(compilation).dependency_contract_template_data(contract);

    let actual = data
        .requirements()
        .iter()
        .filter_map(|requirement| match requirement {
            DependencyRequirement::Direct { subject, kind }
                if subject.subject_root()
                    == DependencySubjectRoot::Parameter(SymbolOrdinal::new(ordinal)) =>
            {
                Some(*kind)
            }
            DependencyRequirement::Direct { .. }
            | DependencyRequirement::Guarded(_)
            | DependencyRequirement::ResultCall { .. }
            | DependencyRequirement::FixedPoint { .. }
            | DependencyRequirement::Variable { .. } => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

pub(super) trait ResolvedTestType {
    fn resolved_type(self) -> TypeId;
}

impl ResolvedTestType for TypeId {
    fn resolved_type(self) -> TypeId {
        self
    }
}

impl ResolvedTestType for &TypeExpressionTemplate {
    fn resolved_type(self) -> TypeId {
        resolved_type(self)
    }
}

pub(super) fn resolved_type(template: &TypeExpressionTemplate) -> TypeId {
    let Some(ty) = template.resolved_type() else {
        panic!("test type template must already be resolved");
    };

    ty
}

fn semantic_values(compilation: &Compilation) -> &bray_symbols::SemanticValueStore {
    match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic value store must be available: {error:?}"),
    }
}
