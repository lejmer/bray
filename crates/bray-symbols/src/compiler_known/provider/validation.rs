use bray_compiler_known::{CompilerKnownDeclarationId, CompilerKnownScopeId};

use super::super::CompilerKnownSymbolBuildError;

pub(super) fn validate_scope_id(
    index: usize,
    actual: CompilerKnownScopeId,
) -> Result<(), CompilerKnownSymbolBuildError> {
    let Some(expected) = CompilerKnownScopeId::try_from_index(index) else {
        return Err(CompilerKnownSymbolBuildError::SymbolCapacityExceeded { index });
    };

    if actual != expected {
        return Err(CompilerKnownSymbolBuildError::NonCanonicalScopeId { expected, actual });
    }

    Ok(())
}

pub(super) fn validate_declaration_id(
    index: usize,
    actual: CompilerKnownDeclarationId,
) -> Result<(), CompilerKnownSymbolBuildError> {
    let Some(expected) = CompilerKnownDeclarationId::try_from_index(index) else {
        return Err(CompilerKnownSymbolBuildError::SymbolCapacityExceeded { index });
    };

    if actual != expected {
        return Err(CompilerKnownSymbolBuildError::NonCanonicalDeclarationId { expected, actual });
    }

    Ok(())
}
