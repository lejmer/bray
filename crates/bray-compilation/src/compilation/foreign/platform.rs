use bray_declarations::DeclarationKind;
use bray_runtime_interface::PlatformServiceRole;
use bray_symbols::FunctionSymbolId;

use super::super::Compilation;
use crate::fact::FactQueryError;

pub(super) fn platform_service_role(
    compilation: &Compilation,
    function: FunctionSymbolId,
) -> Result<Option<PlatformServiceRole>, FactQueryError> {
    let symbols = compilation.symbol_graph()?;

    let declaration = symbols
        .function(function)
        .and_then(bray_symbols::FunctionSymbol::declaration)
        .and_then(|declaration| compilation.declaration_table().declaration(declaration))
        .ok_or(FactQueryError::InfrastructureFailure)?;

    if declaration.kind() != DeclarationKind::Function {
        return Ok(None);
    }

    let Some(name) = declaration
        .name()
        .and_then(bray_declarations::DeclarationName::as_identifier)
    else {
        return Ok(None);
    };

    let module = compilation
        .declaration_table()
        .container(declaration.owning_container())
        .and_then(bray_declarations::ContainerRecord::module_path)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let mut matched = compilation
        .state
        .platform_services
        .iter()
        .filter(|binding| {
            binding.declaration() == name
                && binding.module().eq(module.segments().iter().map(String::as_str))
        });

    let role = matched.next().map(bray_runtime_interface::PlatformServiceBinding::role);

    if matched.next().is_some() {
        return Err(FactQueryError::InfrastructureFailure);
    }

    Ok(role)
}
