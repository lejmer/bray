use bray_declarations::DeclarationRecord;
use bray_symbols::{ModuleOwnerId, ModulePathKey, ModuleSymbol, SymbolGraph};

use super::Compilation;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation) fn source_module_for_declaration<'symbols>(
        &self,
        symbols: &'symbols SymbolGraph,
        declaration: &DeclarationRecord,
    ) -> Result<&'symbols ModuleSymbol, FactQueryError> {
        let container_id = declaration.owning_container();

        let container = self
            .product_source_graph()?
            .declarations()
            .container(container_id)
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Container(container_id),
                    ProductDataKind::DeclarationContainer,
                )
            })?;

        let path = container.module_path().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Container(container_id),
                ProductDataKind::ModulePath,
            )
        })?;

        let segments = path.segments().to_vec();

        let path =
            ModulePathKey::try_new(segments.iter().map(String::as_str)).ok_or_else(|| {
                ProductQueryFailure::InvalidModulePath {
                    declaration: declaration.id(),
                    segments: segments.into_boxed_slice(),
                }
            })?;

        let package = symbols.roots().packages().first().copied().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Declaration(declaration.id()),
                ProductDataKind::PackageRoot,
            )
        })?;

        let owner = ModuleOwnerId::from(package);

        symbols.module_by_path(owner, &path).ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::ModulePath { owner, path },
                ProductDataKind::Module,
            )
            .into()
        })
    }
}
