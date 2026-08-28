use bray_compiler_known::{COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryScopeId};
use bray_symbols::{AnySymbolId, ModuleOwnerId, ModulePathKey, PackageSymbolId, SymbolGraph};

pub(super) fn source_standard_library_scope_owner(
    symbols: &SymbolGraph,
    package: PackageSymbolId,
    scope: RecognizedStandardLibraryScopeId,
) -> Option<AnySymbolId> {
    let scope = COMPILER_KNOWN_CATALOG.recognized_standard_library_scope(scope)?;
    let path = ModulePathKey::try_new(scope.path().segments())?;
    let module = symbols.module_by_path(ModuleOwnerId::from(package), &path)?;

    Some(module.id().into())
}
