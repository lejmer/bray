use bray_declarations::DeclarationId;

use crate::{
    AnySymbolId, MemberCollectionBuildError, MemberEntry, MemberLookupIndex, MemberLookupResult,
    MemberValidity, MemberVisibility, SymbolName,
};

/// One validated module-level using declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleUsing {
    declaration: DeclarationId,
    target: AnySymbolId,
    permits_internal_access: bool,
}

impl ModuleUsing {
    /// Creates one using relationship to an exact declaration identity.
    pub const fn new(
        declaration: DeclarationId,
        target: AnySymbolId,
        permits_internal_access: bool,
    ) -> Self {
        Self {
            declaration,
            target,
            permits_internal_access,
        }
    }

    /// Returns the source declaration that introduced this relationship.
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// Returns the exact declaration named by the using path.
    pub const fn target(&self) -> AnySymbolId {
        self.target
    }

    /// Returns whether the declaration explicitly acknowledges internal access.
    pub const fn permits_internal_access(&self) -> bool {
        self.permits_internal_access
    }
}

/// One validated lookup edge introduced by an export declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleReExport {
    declaration: DeclarationId,
    name: SymbolName,
    target: AnySymbolId,
    visibility: MemberVisibility,
}

impl ModuleReExport {
    /// Creates one re-export that preserves the target declaration identity.
    pub const fn new(
        declaration: DeclarationId,
        name: SymbolName,
        target: AnySymbolId,
        visibility: MemberVisibility,
    ) -> Self {
        Self {
            declaration,
            name,
            target,
            visibility,
        }
    }

    /// Returns the source declaration that introduced this lookup edge.
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// Returns the name exposed through the exporting module.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the exact declaration reached through this lookup edge.
    pub const fn target(&self) -> AnySymbolId {
        self.target
    }

    /// Returns the visibility of this lookup edge.
    pub const fn visibility(&self) -> MemberVisibility {
        self.visibility
    }
}

/// Immutable using relationships and re-export lookup edges for one module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSurface {
    usings: Box<[ModuleUsing]>,
    re_exports: Box<[ModuleReExport]>,
    lookup: MemberLookupIndex<AnySymbolId>,
}

impl ModuleSurface {
    /// Builds one module surface in canonical declaration order.
    pub fn new(
        usings: impl IntoIterator<Item = ModuleUsing>,
        re_exports: impl IntoIterator<Item = ModuleReExport>,
    ) -> Result<Self, MemberCollectionBuildError<AnySymbolId>> {
        let usings = usings.into_iter().collect();
        let re_exports = re_exports.into_iter().collect::<Box<[_]>>();

        let lookup = MemberLookupIndex::new(re_exports.iter().map(|edge| {
            // The record and lookup index share the export name's Arc-backed text.
            MemberEntry::new(
                edge.target,
                edge.name.clone(),
                edge.visibility,
                MemberValidity::Valid,
            )
        }))?;

        Ok(Self {
            usings,
            re_exports,
            lookup,
        })
    }

    /// Returns validated using relationships in canonical declaration order.
    pub fn usings(&self) -> &[ModuleUsing] {
        &self.usings
    }

    /// Returns validated re-export edges in canonical declaration order.
    pub fn re_exports(&self) -> &[ModuleReExport] {
        &self.re_exports
    }

    /// Resolves one re-exported name with internal declarations available.
    pub fn lookup(&self, name: &str) -> MemberLookupResult<AnySymbolId> {
        self.lookup.lookup(name)
    }

    /// Resolves one re-exported name through the public module surface.
    pub fn lookup_public(&self, name: &str) -> MemberLookupResult<AnySymbolId> {
        self.lookup.lookup_public(name)
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;

    use super::{ModuleReExport, ModuleSurface, ModuleUsing};
    use crate::{
        AnySymbolId, FunctionSymbolId, MemberLookupResult, MemberVisibility, SymbolId, SymbolName,
    };

    #[test]
    fn module_surfaces_preserve_relationships_and_lookup_target_identity() {
        let target = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(7)));
        let using = ModuleUsing::new(DeclarationId::new(1), target, true);

        let name =
            SymbolName::try_new("run").unwrap_or_else(|| panic!("test export name must be valid"));

        let export = ModuleReExport::new(
            DeclarationId::new(2),
            name,
            target,
            MemberVisibility::Internal,
        );

        let surface = ModuleSurface::new([using], [export])
            .unwrap_or_else(|error| panic!("test module surface must build: {error:?}"));

        assert_eq!(surface.usings(), &[using]);
        assert_eq!(surface.re_exports()[0].target(), target);
        assert_eq!(surface.lookup("run"), MemberLookupResult::Found(target));

        assert_eq!(
            surface.lookup_public("run"),
            MemberLookupResult::Inaccessible(Box::new([target]))
        );
    }
}
