use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bray_base::Cancellation;
use bray_declarations::{
    DeclarationTable, discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::DiagnosticResult;
use bray_parser::{SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceIdentity, SourceInput, SourceStore, SourceVersion};
use bray_symbols::{
    CallableSignatureFact, CallableSignatureTemplate, ConstantDeclaredTypeFact, ConstantSymbolId,
    ImportedSymbolSkeleton, PackageIdentity, SemanticValueStore, SymbolFactRequest, SymbolGraph,
    TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::SyntaxTree;
use bray_target::TargetProfile;

use super::{
    BinderFactContext, BinderFactError, BinderFactResult, ImportedPathRoot, SymbolFactProvider,
};

pub(crate) struct TestSymbolFacts {
    symbol: ConstantSymbolId,
    pub(super) result: Arc<DiagnosticResult<TypeExpressionTemplate>>,
    callable_signature: Arc<DiagnosticResult<CallableSignatureTemplate>>,
}

impl SymbolFactProvider<ConstantDeclaredTypeFact> for TestSymbolFacts {
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<Arc<DiagnosticResult<TypeExpressionTemplate>>> {
        if request.owner() != self.symbol {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

impl SymbolFactProvider<CallableSignatureFact> for TestSymbolFacts {
    fn symbol_fact(
        &self,
        _: SymbolFactRequest<CallableSignatureFact>,
    ) -> BinderFactResult<Arc<DiagnosticResult<CallableSignatureTemplate>>> {
        Ok(Arc::clone(&self.callable_signature))
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

impl Cancellation for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) struct TestContext<'facts> {
    syntax: &'facts SyntaxTree,
    declarations: &'facts DeclarationTable,
    symbols: &'facts SymbolGraph,
    imported_symbols: Option<&'facts ImportedSymbolSkeleton>,
    semantic_values: &'facts SemanticValueStore,
    target: &'facts TargetProfile,
    symbol_facts: &'facts TestSymbolFacts,
    cancellation: &'facts TestCancellation,
}

impl BinderFactContext for TestContext<'_> {
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

    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        let Some(symbols) = self.imported_symbols else {
            return Ok(None);
        };

        let selected = symbols
            .packages()
            .iter()
            .filter_map(|package| {
                let component_count = package.identity().as_str().split('.').count();

                (component_count <= components.len()
                    && package
                        .identity()
                        .as_str()
                        .split('.')
                        .eq(components[..component_count].iter().copied()))
                .then_some((package, component_count))
            })
            .max_by_key(|(_, component_count)| *component_count);

        Ok(selected
            .and_then(|(package, _)| ImportedPathRoot::for_path(symbols, package.id(), components)))
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn selected_target(&self) -> &TargetProfile {
        self.target
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
    symbol_facts: TestSymbolFacts,
    target: TargetProfile,
    cancellation: TestCancellation,
}

impl TestFixture {
    pub(crate) fn new() -> Self {
        Self::from_source(concat!("module app;\n", "const size: i32 = 1;"))
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

        let symbols = match SymbolGraph::build_source(package, &declarations, &syntax) {
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

        let symbol_result = Arc::new(DiagnosticResult::without_diagnostics(
            TypeExpressionTemplate::Resolved(declared_type),
        ));

        let callable_signature = Arc::new(DiagnosticResult::without_diagnostics(
            CallableSignatureTemplate::new(
                TypeExpressionTemplate::Resolved(declared_type),
                None,
                [],
                TypeExpressionTemplate::Resolved(declared_type),
            ),
        ));

        Self {
            syntax,
            declarations,
            symbols,
            semantic_values,
            constant,
            declared_type,
            symbol_facts: TestSymbolFacts {
                symbol: constant,
                result: symbol_result,
                callable_signature,
            },
            target: bray_target::test_support::test_target_profile(),
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
            imported_symbols: None,
            semantic_values: &self.semantic_values,
            target: &self.target,
            symbol_facts: &self.symbol_facts,
            cancellation: &self.cancellation,
        }
    }

    pub(crate) fn context_with_imported<'fixture>(
        &'fixture self,
        imported_symbols: &'fixture ImportedSymbolSkeleton,
    ) -> TestContext<'fixture> {
        TestContext {
            imported_symbols: Some(imported_symbols),
            ..self.context()
        }
    }
}
