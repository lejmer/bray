use std::collections::BTreeMap;

use bray_compiler_known::CompilerKnownDeclarationId;

use crate::CompilerKnownSymbolBuildError;

pub(super) fn required_declaration_value<T>(
    values: &BTreeMap<CompilerKnownDeclarationId, T>,
    declaration: CompilerKnownDeclarationId,
) -> Result<&T, CompilerKnownSymbolBuildError> {
    values
        .get(&declaration)
        .ok_or(CompilerKnownSymbolBuildError::InvalidDeclarationSurface { declaration })
}
