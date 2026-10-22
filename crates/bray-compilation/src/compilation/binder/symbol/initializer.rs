use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_symbols::StaticSymbolId;

impl crate::compilation::Compilation {
    pub(in crate::compilation) fn static_initializer_key(
        &self,
        declaration: StaticSymbolId,
    ) -> Result<Option<BoundUnitKey>, crate::fact::FactQueryError> {
        self.declared_unit_key(declaration.into(), BoundUnitKind::ConstantTemplate)
    }
}
