use bray_declarations::DeclarationRecord;
use bray_symbols::{ModuleOwnerId, ModulePathKey, ModuleSymbol, SymbolGraph};

use super::Compilation;
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation) fn source_module_for_declaration<'symbols>(
        &self,
        symbols: &'symbols SymbolGraph,
        declaration: &DeclarationRecord,
    ) -> Result<&'symbols ModuleSymbol, FactQueryError> {
        let container = self
            .product_source_graph()?
            .declarations()
            .container(declaration.owning_container())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let path = container
            .module_path()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let path = ModulePathKey::try_new(path.segments().iter().map(String::as_str))
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let package = symbols
            .roots()
            .packages()
            .first()
            .copied()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        symbols
            .module_by_path(ModuleOwnerId::from(package), &path)
            .ok_or(FactQueryError::InfrastructureFailure)
    }
}
