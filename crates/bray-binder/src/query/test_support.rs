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
    AnySymbolId, CallableAbi, CallableConstness, CallableDependencyContracts,
    CallableSignatureQuery, CallableSignatureTemplate, CallableTrust, CallableTypeData,
    ConstantDeclaredTypeQuery, ConstantSymbolId, DependencyContractTemplateData,
    ImportedSymbolSkeleton, MemberLookupResult, ModuleSymbolId, NamedTypeSymbolId, PackageIdentity,
    SemanticValueStore, SymbolGraph, SymbolQueryRequest, TypeAssociatedSurface, TypeData,
    TypeExpressionTemplate, TypeId,
};
use bray_syntax::SyntaxTree;
use bray_target::TargetProfile;

use super::{BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider};
use crate::ImportedPathRoot;

pub(crate) struct TestSymbolSemantics {
    symbol: ConstantSymbolId,
    pub(super) result: Arc<DiagnosticResult<TypeExpressionTemplate>>,
    callable_signature: Arc<DiagnosticResult<CallableSignatureTemplate>>,
}

impl SymbolQueryProvider<ConstantDeclaredTypeQuery> for TestSymbolSemantics {
    fn resolve_symbol_query(
        &self,
        request: SymbolQueryRequest<ConstantDeclaredTypeQuery>,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeExpressionTemplate>>> {
        if request.owner() != self.symbol {
            return Err(BindingQueryError::DependencyUnavailable);
        }

        Ok(Arc::clone(&self.result))
    }
}

impl SymbolQueryProvider<CallableSignatureQuery> for TestSymbolSemantics {
    fn resolve_symbol_query(
        &self,
        _: SymbolQueryRequest<CallableSignatureQuery>,
    ) -> BindingQueryResult<Arc<DiagnosticResult<CallableSignatureTemplate>>> {
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

pub(crate) struct TestContext<'binding_context> {
    syntax: &'binding_context SyntaxTree,
    declarations: &'binding_context DeclarationTable,
    symbols: &'binding_context SymbolGraph,
    imported_symbols: Option<&'binding_context ImportedSymbolSkeleton>,
    semantic_values: &'binding_context SemanticValueStore,
    target: &'binding_context TargetProfile,
    symbol_semantics: &'binding_context TestSymbolSemantics,
    cancellation: &'binding_context TestCancellation,
}

impl BindingQueryContext for TestContext<'_> {
    type SymbolSemantics = TestSymbolSemantics;
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
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>> {
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

    fn imported_symbols(&self) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>> {
        Ok(self.imported_symbols)
    }

    fn module_re_export_lookup(
        &self,
        _module: ModuleSymbolId,
        _name: &str,
        _access: crate::NameAccess,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>> {
        Ok(MemberLookupResult::NotFound)
    }

    fn type_associated_surface(
        &self,
        _subject: NamedTypeSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeAssociatedSurface>>> {
        Err(BindingQueryError::DependencyUnavailable)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn selected_target(&self) -> &TargetProfile {
        self.target
    }

    fn symbol_semantics(&self) -> &Self::SymbolSemantics {
        self.symbol_semantics
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
    symbol_semantics: TestSymbolSemantics,
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
            "binder-binding_context",
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

        let dependency = match semantic_values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        {
            Ok(dependency) => dependency,
            Err(error) => panic!("test dependency contract should intern: {error:?}"),
        };

        let callable_type = CallableTypeData::new(
            [],
            declared_type,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::Bray,
            CallableDependencyContracts::synchronous(dependency),
        );

        let callable_type = match semantic_values.intern_type(TypeData::Callable(callable_type)) {
            Ok(callable_type) => callable_type,
            Err(error) => panic!("test callable type should intern: {error:?}"),
        };

        let callable_signature = Arc::new(DiagnosticResult::without_diagnostics(
            CallableSignatureTemplate::new(
                TypeExpressionTemplate::Resolved(callable_type),
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
            symbol_semantics: TestSymbolSemantics {
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
            symbol_semantics: &self.symbol_semantics,
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
