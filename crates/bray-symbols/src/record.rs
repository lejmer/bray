mod declaration;
mod provider;
mod root;

// bray-style: allow(wildcard-import, reason = "declaration record names are defined by a centralized macro inventory")
pub use declaration::*;
pub(crate) use declaration::{
    DeclarationSymbolIdentity, ImportedSymbolBacking, for_each_declaration_symbol,
};
// bray-style: allow(wildcard-import, reason = "default-provider record names are defined by macros")
pub use provider::*;
pub(crate) use root::ModuleSymbolInput;
pub use root::{CompilerKnownEnvironmentSymbol, ModuleSymbol, PackageSymbol};
