use bray_bound_tree::{BoundUnit, BoundUnitId, BoundUnitKind, CheckedControlFlowFacts};

/// A validated borrowed view of the completed checked HIR required by lowering.
///
/// The view keeps the canonical bound unit and its independently published semantic facts
/// separate. Adding another required checker domain extends this input rather than creating a
/// copied or progressively wrapped bound-tree representation.
#[derive(Clone, Copy)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    control_flow: &'unit CheckedControlFlowFacts,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
    ) -> Result<Self, LoweringInputError> {
        if control_flow.unit() != unit.unit() {
            return Err(LoweringInputError::ForeignControlFlow {
                expected: unit.unit(),
                actual: control_flow.unit(),
            });
        }

        if control_flow.kind() != unit.key().kind() {
            return Err(LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: control_flow.kind(),
            });
        }

        Ok(Self { unit, control_flow })
    }

    /// Returns the canonical checked source-shaped semantic unit.
    pub const fn unit(self) -> &'unit BoundUnit {
        self.unit
    }

    /// Returns the durable control-flow facts established for the unit.
    pub const fn control_flow(self) -> &'unit CheckedControlFlowFacts {
        self.control_flow
    }
}

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringInputError {
    /// Control-flow facts belong to another compilation-local bound unit.
    ForeignControlFlow {
        /// The canonical bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied control-flow facts.
        actual: BoundUnitId,
    },
    /// Control-flow facts describe another semantic unit category.
    ControlFlowKindMismatch {
        /// The category carried by the canonical bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied control-flow facts.
        actual: BoundUnitKind,
    },
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundCallableBody, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder, BoundUnit,
        BoundUnitId, BoundUnitKey, BoundUnitRoot, CheckedControlFlowFacts, ControlCompletion,
    };
    use bray_declarations::{DeclarationId, discover_source_unit_declarations};
    use bray_parser::parse_source_unit;
    use bray_source::{
        SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextSize,
    };
    use bray_symbols::{
        LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
        LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SymbolKey,
        SymbolKind, SymbolRootKey,
    };

    use super::{LoweringInput, LoweringInputError};

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_facts() {
        let unit = bound_unit(4);
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let input = match LoweringInput::try_new(&unit, &control_flow) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
    }

    #[test]
    fn input_rejects_foreign_and_wrong_category_side_facts() {
        let unit = bound_unit(4);

        let foreign = CheckedControlFlowFacts::new(
            BoundUnitId::new(5),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(&unit, &foreign),
            LoweringInputError::ForeignControlFlow {
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(5),
            },
        );

        let wrong_kind = CheckedControlFlowFacts::new(
            unit.unit(),
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(&unit, &wrong_kind),
            LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
            },
        );
    }

    #[test]
    fn input_is_safe_to_share_between_lowering_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LoweringInput<'static>>();
    }

    fn assert_input_error(
        result: Result<LoweringInput<'_>, LoweringInputError>,
        expected: LoweringInputError,
    ) {
        let error = match result {
            Ok(_) => panic!("invalid lowering input must be rejected"),
            Err(error) => error,
        };

        assert_eq!(error, expected);
    }

    fn bound_unit(unit: u32) -> BoundUnit {
        let (key, local_symbols) = unit_identity(unit);
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit));
        let body = BoundCallableBody::error(BoundNodeOrigin::source(key.source()), None);

        let root = match tree.push_callable_body(body) {
            Ok(root) => root,
            Err(error) => panic!("test callable body must fit: {error:?}"),
        };

        match BoundUnit::try_new(
            key,
            tree.finish(),
            local_symbols,
            [],
            BoundUnitRoot::CallableBody(root),
        ) {
            Ok(unit) => unit,
            Err(error) => panic!("test bound unit must validate: {error:?}"),
        }
    }

    fn unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
        let snapshot = source();
        let parsed = parse_source_unit(&snapshot);

        assert!(parsed.diagnostics().is_empty());

        let declarations = discover_source_unit_declarations(parsed.source_unit());

        assert!(declarations.diagnostics().is_empty());

        let [part] = declarations.chunk().module_parts() else {
            panic!("test source must contain one module part");
        };

        let source = BoundSourceAnchor::new(part.syntax_anchor(), snapshot.version());
        let owner = function_key();

        let Some(key) = BoundUnitKey::callable_body(owner.clone(), source) else {
            panic!("function must support a callable body");
        };

        let Some(region_key) = LocalSymbolRegionKey::try_new(
            owner,
            LocalSymbolRegionRole::CallableBody,
            [source.syntax()],
            None,
        ) else {
            panic!("test local symbol region key must be valid");
        };

        let mut symbols =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit), region_key);

        if let Err(error) = symbols.push_scope(
            None,
            LocalScopeBoundary::Root,
            source.syntax(),
            TextSize::ZERO,
        ) {
            panic!("test root scope must validate: {error:?}");
        }

        let symbols = match symbols.finish() {
            Ok(symbols) => symbols,
            Err(error) => panic!("test local symbols must validate: {error:?}"),
        };

        (key, symbols)
    }

    fn function_key() -> SymbolKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be valid");
        };

        let Some(path) = ModulePathKey::try_new(["example"]) else {
            panic!("test module path must be valid");
        };

        let module = SymbolKey::module(SymbolRootKey::Package(package), path);

        let Some(function) =
            SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
        else {
            panic!("function symbols must be source-declared");
        };

        function
    }

    fn source() -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("lowering-test"),
            SourceVersion::new(1),
            "module example;",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source must fit: {error:?}"),
        }
    }
}
