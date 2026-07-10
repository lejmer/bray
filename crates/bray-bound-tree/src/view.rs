use crate::{BoundUnitId, BoundUnitKey, BoundUnitKind};

/// A read-only view of committed task-local bound structure before publication.
///
/// The view borrows the stable unit key so checker services cannot retain or
/// mutate binder-owned construction state.
// TODO(bound-tree): Add read-only bound node and side-table accessors as those
// representations land.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundUnitView<'unit> {
    unit: BoundUnitId,
    key: &'unit BoundUnitKey,
}

impl<'unit> BoundUnitView<'unit> {
    /// Creates a read-only view for one exact semantic unit.
    pub const fn new(unit: BoundUnitId, key: &'unit BoundUnitKey) -> Self {
        Self { unit, key }
    }

    /// Returns the compilation-local identity of the bound unit.
    pub const fn unit(self) -> BoundUnitId {
        self.unit
    }

    /// Returns the stable construction key of the bound unit.
    pub const fn key(self) -> &'unit BoundUnitKey {
        self.key
    }

    /// Returns the exact semantic category of the bound unit.
    pub fn kind(self) -> BoundUnitKind {
        self.key.kind()
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;
    use bray_symbols::{ModulePathKey, PackageIdentity, SymbolKey, SymbolKind, SymbolRootKey};

    use super::BoundUnitView;
    use crate::test_support::source_anchor;
    use crate::{BoundUnitId, BoundUnitKey, BoundUnitKind};

    #[test]
    fn views_borrow_exact_unit_identity_without_cloning_keys() {
        let key = callable_body_key();
        let view = BoundUnitView::new(BoundUnitId::new(7), &key);

        assert_eq!(view.unit(), BoundUnitId::new(7));
        assert!(std::ptr::eq(view.key(), &key));
        assert_eq!(view.kind(), BoundUnitKind::CallableBody);
    }

    #[test]
    fn views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<BoundUnitView<'static>>();
    }

    fn callable_body_key() -> BoundUnitKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity is non-empty");
        };

        let Some(path) = ModulePathKey::try_new(["example"]) else {
            panic!("test module path is non-empty");
        };

        let module = SymbolKey::module(SymbolRootKey::Package(package), path);

        let Some(owner) =
            SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
        else {
            panic!("functions can own source declarations");
        };

        let Some(key) = BoundUnitKey::callable_body(owner, source_anchor()) else {
            panic!("functions can own callable bodies");
        };

        key
    }
}
