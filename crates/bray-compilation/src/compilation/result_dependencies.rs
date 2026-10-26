#[cfg(test)]
pub(super) fn source_function_dependencies(
    compilation: &crate::Compilation,
    name: &str,
) -> std::sync::Arc<bray_symbols::DependencyContractTemplateData> {
    let callable = crate::test_support::source_function(compilation, name);

    let context = compilation
        .binding_context(&compilation.state.cancellation)
        .expect("fixture compilation must provide a binding context");

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .expect("fixture symbol query must resolve");

    compilation
        .semantic_value_store()
        .expect("fixture compilation must provide its semantic value store")
        .dependency_contract_template_data(*dependencies.value())
}
