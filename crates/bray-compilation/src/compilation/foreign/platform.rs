use bray_runtime_interface::PlatformServiceRole;
use bray_symbols::FunctionSymbolId;

use super::super::Compilation;
use super::ForeignSourceRole;
use crate::fact::FactQueryError;

pub(in crate::compilation) fn platform_service_role(
    compilation: &Compilation,
    function: FunctionSymbolId,
) -> Result<Option<PlatformServiceRole>, FactQueryError> {
    super::source_role::source_role(
        compilation,
        function,
        compilation.platform_services(),
        ForeignSourceRole::Platform,
    )
}
