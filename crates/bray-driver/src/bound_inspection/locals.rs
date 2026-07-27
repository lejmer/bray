//! Unit-local symbol inspection records.

use bray_symbols::LocalSymbolSnapshot;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct InspectionLocals {
    region: u32,
    pub(super) scope_count: usize,
    pub(super) symbols: Vec<InspectionLocalSymbol>,
}

impl InspectionLocals {
    pub(super) fn from_snapshot(snapshot: &LocalSymbolSnapshot) -> Self {
        let mut symbols = Vec::new();

        for symbol in snapshot.bindings() {
            symbols.push(InspectionLocalSymbol::named(
                "local_binding",
                symbol.id().ordinal(),
                symbol.name().as_str(),
                symbol.is_recovered(),
            ));
        }

        for symbol in snapshot.constants() {
            symbols.push(InspectionLocalSymbol::named(
                "local_constant",
                symbol.id().ordinal(),
                symbol.name().as_str(),
                symbol.is_recovered(),
            ));
        }

        for symbol in snapshot.anonymous_callables() {
            symbols.push(InspectionLocalSymbol::anonymous(
                "anonymous_callable",
                symbol.id().ordinal(),
                symbol.is_recovered(),
            ));
        }

        for symbol in snapshot.anonymous_parameters() {
            symbols.push(InspectionLocalSymbol::named(
                "anonymous_callable_parameter",
                symbol.id().ordinal(),
                symbol.name().as_str(),
                symbol.is_recovered(),
            ));
        }

        for symbol in snapshot.postcondition_results() {
            symbols.push(InspectionLocalSymbol::anonymous(
                "postcondition_result",
                symbol.id().ordinal(),
                symbol.is_recovered(),
            ));
        }

        Self {
            region: snapshot.region().raw(),
            scope_count: snapshot.scopes().len(),
            symbols,
        }
    }
}

#[derive(Serialize)]
pub(super) struct InspectionLocalSymbol {
    pub(super) symbol_kind: &'static str,
    pub(super) id: u32,
    pub(super) name: Option<String>,
    pub(super) recovered: bool,
}

impl InspectionLocalSymbol {
    fn named(symbol_kind: &'static str, id: u32, name: &str, recovered: bool) -> Self {
        Self {
            symbol_kind,
            id,
            name: Some(name.to_owned()),
            recovered,
        }
    }

    const fn anonymous(symbol_kind: &'static str, id: u32, recovered: bool) -> Self {
        Self {
            symbol_kind,
            id,
            name: None,
            recovered,
        }
    }
}
