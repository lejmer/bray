use bray_checker::CheckerRequestContext;
use bray_compiler_known::ImplementationHook;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::FunctionSymbolId;

use super::super::Compilation;
use crate::fact::FactQueryError;

pub(in crate::compilation) fn runtime_role(
    compilation: &Compilation,
    function: FunctionSymbolId,
) -> Result<Option<RuntimeAbiRole>, FactQueryError> {
    super::source_role::source_role(compilation, function, compilation.runtime_roles())
}

pub(super) fn runtime_import_role(
    compilation: &Compilation,
    function: FunctionSymbolId,
    cancellation: &crate::fact::CancellationToken,
) -> Result<Option<RuntimeAbiRole>, FactQueryError> {
    if let Some(role) = runtime_role(compilation, function)? {
        return Ok(Some(role));
    }

    let hook = compilation
        .checker_context(cancellation)?
        .implementation_hook(function.into())
        .map_err(FactQueryError::from)?;

    Ok(
        match hook
            .filter(|hook| hook.is_available())
            .map(|hook| hook.hook())
        {
            Some(ImplementationHook::NativeThreadExecution) => {
                Some(RuntimeAbiRole::NativeThreadExecution)
            }
            _ => None,
        },
    )
}
