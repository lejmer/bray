use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bray_declarations::{
    DeclarationTable, discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::DiagnosticResult;
use bray_parser::{SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceIdentity, SourceInput, SourceStore, SourceVersion};
use bray_symbols::{
    ConstantDeclaredTypeFact, ConstantSymbolId, ConstantValueData, ConstantValueId,
    ConstantValueKind, PackageIdentity, SemanticValueStore, SymbolFactRequest, SymbolGraph,
    TypeData, TypeId,
};
use bray_syntax::SyntaxTree;

use super::{
    BinderCancellation, BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider,
    TargetFactProvider, TargetFactResult,
};

pub(crate) struct TestSymbolFacts {
    symbol: ConstantSymbolId,
    pub(super) result: Arc<DiagnosticResult<TypeId>>,
}

impl SymbolFactProvider<ConstantDeclaredTypeFact> for TestSymbolFacts {
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<Arc<DiagnosticResult<TypeId>>> {
        if request.owner() != self.symbol {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

pub(crate) struct TestTargetFacts {
    symbol: ConstantSymbolId,
    result: Arc<TargetFactResult>,
}

impl TargetFactProvider for TestTargetFacts {
    fn target_fact(&self, fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>> {
        if fact != self.symbol {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

pub(crate) struct TestCancellation {
    cancelled: AtomicBool,
}

impl TestCancellation {
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl BinderCancellation for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) struct TestContext<'facts> {
    syntax: &'facts SyntaxTree,
    declarations: &'facts DeclarationTable,
    symbols: &'facts SymbolGraph,
    semantic_values: &'facts SemanticValueStore,
    target_facts: &'facts TestTargetFacts,
    symbol_facts: &'facts TestSymbolFacts,
    cancellation: &'facts TestCancellation,
}

impl BinderFactContext for TestContext<'_> {
    type TargetFacts = TestTargetFacts;
    type SymbolFacts = TestSymbolFacts;
    type Cancellation = TestCancellation;

    fn syntax(&self) -> &SyntaxTree {
        self.syntax
    }

    fn declarations(&self) -> &DeclarationTable {
        self.declarations
    }

    fn symbols(&self) -> &SymbolGraph {
        self.symbols
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        self.target_facts
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self.symbol_facts
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

pub(crate) struct TestFixture {
    syntax: SyntaxTree,
    declarations: DeclarationTable,
    symbols: SymbolGraph,
    pub(crate) semantic_values: SemanticValueStore,
    pub(crate) constant: ConstantSymbolId,
    pub(crate) declared_type: TypeId,
    pub(crate) target_value: ConstantValueId,
    symbol_facts: TestSymbolFacts,
    target_facts: TestTargetFacts,
    cancellation: TestCancellation,
}

impl TestFixture {
    pub(crate) fn new() -> Self {
        Self::from_source(concat!("module app;\n", "const Size: i32 = 1;"))
    }

    pub(crate) fn from_source(source_text: &str) -> Self {
        let mut sources = SourceStore::new();

        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "binder-facts",
            SourceVersion::new(0),
            source_text,
        );

        let source_id = match sources.insert_input(source) {
            Ok(source_id) => source_id,
            Err(error) => panic!("test source should load: {error:?}"),
        };

        let snapshot = match sources.get(source_id) {
            Some(snapshot) => snapshot,
            None => panic!("inserted test source should exist"),
        };

        let source_syntax = parse_source_unit(snapshot);
        let declaration_chunk = discover_source_unit_declarations(source_syntax.source_unit());
        let (syntax, _) = SyntaxTreeResult::from_source_unit_results([source_syntax]).into_parts();
        let (declarations, _) = merge_declaration_chunks([&declaration_chunk]).into_parts();

        let package = match PackageIdentity::try_new("test.package") {
            Some(package) => package,
            None => panic!("test package identity should be valid"),
        };

        let symbols = match SymbolGraph::build_source(package, &declarations) {
            Ok(symbols) => symbols,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        };

        let Some(constant) = symbols
            .constants()
            .iter()
            .find(|constant| constant.origin() == bray_symbols::SymbolOrigin::Source)
        else {
            panic!("test graph should contain a source constant");
        };

        let constant = constant.id();

        let semantic_values = match SemanticValueStore::try_new() {
            Ok(store) => store,
            Err(error) => panic!("test semantic store should be available: {error:?}"),
        };

        let declared_type = match semantic_values.intern_type(TypeData::Error) {
            Ok(ty) => ty,
            Err(error) => panic!("test type should intern: {error:?}"),
        };

        let target_value = match semantic_values.intern_constant_value(ConstantValueData::new(
            declared_type,
            ConstantValueKind::Error,
        )) {
            Ok(value) => value,
            Err(error) => panic!("test target value should intern: {error:?}"),
        };

        let symbol_result = Arc::new(DiagnosticResult::without_diagnostics(declared_type));
        let target_result = Arc::new(DiagnosticResult::without_diagnostics(target_value));

        Self {
            syntax,
            declarations,
            symbols,
            semantic_values,
            constant,
            declared_type,
            target_value,
            symbol_facts: TestSymbolFacts {
                symbol: constant,
                result: symbol_result,
            },
            target_facts: TestTargetFacts {
                symbol: constant,
                result: target_result,
            },
            cancellation: TestCancellation {
                cancelled: AtomicBool::new(false),
            },
        }
    }

    pub(crate) fn context(&self) -> TestContext<'_> {
        TestContext {
            syntax: &self.syntax,
            declarations: &self.declarations,
            symbols: &self.symbols,
            semantic_values: &self.semantic_values,
            target_facts: &self.target_facts,
            symbol_facts: &self.symbol_facts,
            cancellation: &self.cancellation,
        }
    }
}
