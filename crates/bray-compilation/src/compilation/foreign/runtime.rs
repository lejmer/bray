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
