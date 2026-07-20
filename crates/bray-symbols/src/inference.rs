use std::collections::BTreeMap;

use bray_declarations::{
    ContainerId, DeclarationId, DeclarationKind, DeclarationTable, SyntaxAnchor,
};
use bray_syntax::{
    ExpressionSyntax, GenericArgumentListSyntax, GenericArgumentSyntax,
    InherentImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntax, PathSyntax,
    SourceSyntaxNode, SyntaxCast, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent,
    TypeExpressionSyntax, UnnamedTraitImplementationDeclarationSyntax, walk_syntax_node,
};

use crate::allocator::SymbolIdAllocator;
use crate::build::OwnerIdentity;
use crate::graph::SymbolGraphBuilder;
use crate::record::DeclarationSymbolIdentity;
use crate::{
    AnySymbolId, GenericConstParameterSymbolId, GenericParameterSymbolId,
    GenericTypeParameterSymbolId, MemberLookupResult, MemberValidity, SymbolGraphBuildError,
    SymbolName, SymbolOrdinal, SynthesizedSymbolKey,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParameterKind {
    Type,
    Const,
}

struct InferredParameter {
    name: SymbolName,
    kind: ParameterKind,
    sources: Vec<SyntaxAnchor>,
}

pub(crate) fn push_inferred_implementation_parameters(
    graph: &mut SymbolGraphBuilder,
    declarations: &DeclarationTable,
    syntax: &SyntaxTree,
    module_owners: &BTreeMap<ContainerId, OwnerIdentity>,
    source_identities: &BTreeMap<DeclarationId, OwnerIdentity>,
    allocator: &mut SymbolIdAllocator,
) -> Result<(), SymbolGraphBuildError> {
    for declaration in declarations.declarations() {
        if !is_implementation(declaration.kind()) {
            continue;
        }

        let owner = source_identities.get(&declaration.id()).ok_or(
            SymbolGraphBuildError::MissingSourceSymbol {
                declaration: declaration.id(),
            },
        )?;

        let module = module_owners.get(&declaration.owning_container()).ok_or(
            SymbolGraphBuildError::MissingModuleOwner {
                declaration: declaration.id(),
                container: declaration.owning_container(),
            },
        )?;

        let lookup = HeaderLookup::new(graph, module.id);
        let parameters = infer_parameters(declaration, syntax, &lookup);

        for (ordinal, parameter) in parameters.into_iter().enumerate() {
            let ordinal =
                SymbolOrdinal::new(u32::try_from(ordinal).map_err(|_| {
                    SymbolGraphBuildError::SymbolCapacityExceeded { index: ordinal }
                })?);

            let raw_id = allocator.next()?;

            // Synthesized keys share the implementation's immutable key storage.
            let (id, synthesized_key) = match parameter.kind {
                ParameterKind::Type => (
                    GenericParameterSymbolId::from(GenericTypeParameterSymbolId::from_symbol_id(
                        raw_id,
                    )),
                    SynthesizedSymbolKey::inferred_implementation_type_parameter(
                        owner.key.clone(),
                        ordinal,
                    ),
                ),
                ParameterKind::Const => (
                    GenericParameterSymbolId::from(GenericConstParameterSymbolId::from_symbol_id(
                        raw_id,
                    )),
                    SynthesizedSymbolKey::inferred_implementation_const_parameter(
                        owner.key.clone(),
                        ordinal,
                    ),
                ),
            };

            let identity = DeclarationSymbolIdentity::inferred_implementation_parameter(
                crate::SymbolKey::synthesized(synthesized_key),
                owner.id,
                parameter.name,
                parameter.sources,
            );

            if let Err(symbol_kind) = graph.push_inferred_parameter(id, identity) {
                return Err(SymbolGraphBuildError::InvalidSourceSymbolKind {
                    declaration: declaration.id(),
                    declaration_kind: declaration.kind(),
                    symbol_kind,
                });
            }
        }
    }

    Ok(())
}

fn is_implementation(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::InherentImplementation
            | DeclarationKind::UnnamedTraitImplementation
            | DeclarationKind::NamedTraitImplementation
    )
}

fn infer_parameters(
    declaration: &bray_declarations::DeclarationRecord,
    syntax: &SyntaxTree,
    lookup: &HeaderLookup<'_>,
) -> Vec<InferredParameter> {
    let mut collector = ParameterCollector::new(lookup);
    let anchor = declaration.syntax_anchor();

    match declaration.kind() {
        DeclarationKind::InherentImplementation => {
            if let Some(declaration) =
                anchor.find_descendant::<InherentImplementationDeclarationSyntax>(syntax)
            {
                collector.collect_subject(&declaration.implementation_subject());
            }
        }
        DeclarationKind::UnnamedTraitImplementation => {
            if let Some(declaration) =
                anchor.find_descendant::<UnnamedTraitImplementationDeclarationSyntax>(syntax)
            {
                collector.collect_subject(&declaration.implementation_subject());
                collector.collect_trait_application(&declaration.trait_application());
            }
        }
        DeclarationKind::NamedTraitImplementation => {
            if let Some(declaration) =
                anchor.find_descendant::<NamedTraitImplementationDeclarationSyntax>(syntax)
            {
                collector.collect_subject(&declaration.implementation_subject());
                collector.collect_trait_application(&declaration.trait_application());
            }
        }
        _ => {}
    }

    collector.finish()
}

struct HeaderLookup<'graph> {
    graph: &'graph SymbolGraphBuilder,
    source_names: BTreeMap<&'graph str, HeaderNameResolution>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeaderNameResolution {
    NotFound,
    Unique(AnySymbolId),
    Occupied,
}

impl<'graph> HeaderLookup<'graph> {
    fn new(graph: &'graph SymbolGraphBuilder, module: AnySymbolId) -> Self {
        let mut source_names = BTreeMap::new();

        for entry in graph.source_member_entries(module) {
            let resolution = if entry.validity() == MemberValidity::Valid {
                HeaderNameResolution::Unique(entry.id())
            } else {
                HeaderNameResolution::Occupied
            };

            source_names
                .entry(entry.name().as_str())
                .and_modify(|current| {
                    *current = combine_name_resolutions(*current, resolution);
                })
                .or_insert(resolution);
        }

        Self {
            graph,
            source_names,
        }
    }

    fn resolve_name(&self, name: &str) -> HeaderNameResolution {
        let source = self
            .source_names
            .get(name)
            .copied()
            .unwrap_or(HeaderNameResolution::NotFound);

        let compiler_known = self.graph.compiler_known_provider();
        let ambient = compiler_known.environment().id().into();
        let compiler_known =
            match compiler_known.lookup_member_with_access(ambient, name, |_, _| true) {
                MemberLookupResult::Found(candidate) => HeaderNameResolution::Unique(candidate),
                MemberLookupResult::NotFound => HeaderNameResolution::NotFound,
                MemberLookupResult::WrongKind(_)
                | MemberLookupResult::Ambiguous(_)
                | MemberLookupResult::Inaccessible(_)
                | MemberLookupResult::Malformed(_) => HeaderNameResolution::Occupied,
            };

        combine_name_resolutions(source, compiler_known)
    }

    fn generic_parameter_kinds(&self, owner: AnySymbolId) -> Vec<ParameterKind> {
        let source_children = self.graph.relationship_children(owner);
        let children = if source_children.is_empty() {
            self.graph
                .compiler_known_provider()
                .completion_children(owner)
        } else {
            source_children
        };

        children
            .iter()
            .filter_map(|child| match child {
                AnySymbolId::GenericTypeParameter(_) => Some(ParameterKind::Type),
                AnySymbolId::GenericConstParameter(_) => Some(ParameterKind::Const),
                _ => None,
            })
            .collect()
    }
}

fn combine_name_resolutions(
    first: HeaderNameResolution,
    second: HeaderNameResolution,
) -> HeaderNameResolution {
    match (first, second) {
        (HeaderNameResolution::NotFound, result) | (result, HeaderNameResolution::NotFound) => {
            result
        }
        (HeaderNameResolution::Unique(_), HeaderNameResolution::Unique(_))
        | (HeaderNameResolution::Unique(_), HeaderNameResolution::Occupied)
        | (HeaderNameResolution::Occupied, HeaderNameResolution::Unique(_))
        | (HeaderNameResolution::Occupied, HeaderNameResolution::Occupied) => {
            HeaderNameResolution::Occupied
        }
    }
}

struct ParameterCollector<'lookup> {
    lookup: &'lookup HeaderLookup<'lookup>,
    indexes: BTreeMap<SymbolName, usize>,
    parameters: Vec<InferredParameter>,
}

impl<'lookup> ParameterCollector<'lookup> {
    fn new(lookup: &'lookup HeaderLookup<'lookup>) -> Self {
        Self {
            lookup,
            indexes: BTreeMap::new(),
            parameters: Vec::new(),
        }
    }

    fn collect_subject(&mut self, subject: &bray_syntax::ImplementationSubjectSyntax) {
        let mut generic_arguments = subject.generic_argument_lists().peekable();
        let target = self.resolve_path(&subject.path());

        if target.is_none() && generic_arguments.peek().is_none() {
            self.collect_path(&subject.path(), ParameterKind::Type);
        }

        for arguments in generic_arguments {
            self.collect_arguments(arguments, target);
        }
    }

    fn collect_trait_application(&mut self, application: &bray_syntax::TraitApplicationSyntax) {
        let target = self.resolve_path(&application.path());

        for arguments in application.generic_argument_lists() {
            self.collect_arguments(arguments, target);
        }
    }

    fn collect_type_expression(&mut self, expression: TypeExpressionSyntax) {
        let target = expression.path().and_then(|path| self.resolve_path(&path));

        if let Some(path) = expression.path()
            && target.is_none()
            && expression.generic_argument_lists().next().is_none()
        {
            self.collect_path(&path, ParameterKind::Type);
        }

        for arguments in expression.generic_argument_lists() {
            self.collect_arguments(arguments, target);
        }

        for nested in expression.type_expressions() {
            self.collect_type_expression(nested);
        }
    }

    fn collect_arguments(
        &mut self,
        arguments: GenericArgumentListSyntax,
        target: Option<AnySymbolId>,
    ) {
        let kinds = target
            .map(|target| self.lookup.generic_parameter_kinds(target))
            .unwrap_or_default();

        for (ordinal, argument) in arguments.generic_arguments().enumerate() {
            self.collect_argument(argument, kinds.get(ordinal).copied());
        }
    }

    fn collect_argument(&mut self, argument: GenericArgumentSyntax, kind: Option<ParameterKind>) {
        match kind {
            Some(ParameterKind::Type) => {
                for expression in argument.type_expressions() {
                    self.collect_type_expression(expression);
                }
            }
            Some(ParameterKind::Const) => self.collect_constant_argument(&argument),
            None => {
                for expression in argument.type_expressions() {
                    self.collect_type_expression(expression);
                }

                for expression in argument.expressions() {
                    self.collect_constant_expression(&expression);
                }
            }
        }
    }

    fn collect_constant_argument(&mut self, argument: &GenericArgumentSyntax) {
        for expression in argument.type_expressions() {
            self.collect_paths(&expression, ParameterKind::Const);
        }

        for expression in argument.expressions() {
            self.collect_constant_expression(&expression);
        }
    }

    fn collect_constant_expression(&mut self, expression: &ExpressionSyntax) {
        self.collect_paths(expression, ParameterKind::Const);
    }

    fn collect_paths(&mut self, syntax: &impl bray_syntax::SyntaxWalkRoot, kind: ParameterKind) {
        walk_syntax_node(syntax, |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && let Some(path) = PathSyntax::cast_from(node)
            {
                self.collect_path(&path, kind);

                return SyntaxWalkControl::SkipChildren;
            }

            SyntaxWalkControl::Continue
        });
    }

    fn collect_path(&mut self, path: &PathSyntax, kind: ParameterKind) {
        let Some(text) = path_text(path) else {
            return;
        };

        let Some(name) = SymbolName::try_new(text) else {
            return;
        };

        let source = SyntaxAnchor::from_node(path);

        if let Some(index) = self.indexes.get(&name).copied() {
            self.parameters[index].sources.push(source);

            return;
        }

        if self.lookup.resolve_name(text) != HeaderNameResolution::NotFound {
            return;
        }

        // The index and record share the name's immutable text storage.
        self.indexes.insert(name.clone(), self.parameters.len());
        self.parameters.push(InferredParameter {
            name,
            kind,
            sources: vec![source],
        });
    }

    fn resolve_path(&self, path: &PathSyntax) -> Option<AnySymbolId> {
        let name = path_text(path)?;

        match self.lookup.resolve_name(name) {
            HeaderNameResolution::Unique(candidate) => Some(candidate),
            HeaderNameResolution::NotFound | HeaderNameResolution::Occupied => None,
        }
    }

    fn finish(self) -> Vec<InferredParameter> {
        self.parameters
    }
}

fn path_text(path: &PathSyntax) -> Option<&str> {
    let mut identifiers = path.identifier_tokens();
    let identifier = identifiers.next()?;

    if identifiers.next().is_some() {
        return None;
    }

    identifier.text(path.source().text())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        PackageIdentity, SymbolGraph, SymbolKeyData, SymbolOrdinal, SymbolOrigin,
        SynthesizedSymbolRole,
    };

    #[test]
    fn unresolved_implementation_names_publish_typed_parameter_identities() {
        let graph = graph(&[concat!(
            "module app;\n",
            "struct Buffer<T, const count: usize>\n",
            "{\n",
            "}\n",
            "trait Comparable<T>\n",
            "{\n",
            "}\n",
            "impl Buffer<T, count>(Comparable<T>)\n",
            "{\n",
            "}\n",
        )]);

        let implementation = source_unnamed_trait_implementation(&graph);
        let [type_parameter] = implementation.generic_type_parameters() else {
            panic!("implementation should infer one type parameter");
        };
        let [const_parameter] = implementation.generic_const_parameters() else {
            panic!("implementation should infer one constant parameter");
        };

        let Some(type_parameter) = graph.generic_type_parameter(*type_parameter) else {
            panic!("inferred type parameter ID should resolve");
        };
        let Some(const_parameter) = graph.generic_const_parameter(*const_parameter) else {
            panic!("inferred constant parameter ID should resolve");
        };

        assert_eq!(
            type_parameter.inferred_name().map(|name| name.as_str()),
            Some("T")
        );
        assert_eq!(
            const_parameter.inferred_name().map(|name| name.as_str()),
            Some("count")
        );
        assert_eq!(type_parameter.origin(), SymbolOrigin::Synthesized);
        assert_eq!(const_parameter.origin(), SymbolOrigin::Synthesized);
        assert_eq!(type_parameter.ordinal(), 0);
        assert_eq!(const_parameter.ordinal(), 1);
        assert_eq!(type_parameter.inference_sources().len(), 2);
        assert_eq!(const_parameter.inference_sources().len(), 1);

        let SymbolKeyData::Synthesized(type_key) = type_parameter.key().data() else {
            panic!("inferred type parameter should use a synthesized key");
        };
        let SymbolKeyData::Synthesized(const_key) = const_parameter.key().data() else {
            panic!("inferred constant parameter should use a synthesized key");
        };

        assert_eq!(
            type_key.role(),
            SynthesizedSymbolRole::InferredImplementationTypeParameter
        );
        assert_eq!(type_key.ordinal(), Some(SymbolOrdinal::new(0)));
        assert_eq!(
            const_key.role(),
            SynthesizedSymbolRole::InferredImplementationConstParameter
        );
        assert_eq!(const_key.ordinal(), Some(SymbolOrdinal::new(1)));
    }

    #[test]
    fn ordinary_source_and_compiler_known_names_are_not_inferred() {
        let graph = graph(&[concat!(
            "module app;\n",
            "struct Existing\n",
            "{\n",
            "}\n",
            "const width: usize = 1;\n",
            "struct Buffer<T, const count: usize>\n",
            "{\n",
            "}\n",
            "impl Buffer<Existing, width>\n",
            "{\n",
            "}\n",
            "impl Buffer<i32, 1>\n",
            "{\n",
            "}\n",
        )]);

        let implementations = graph
            .inherent_implementations()
            .iter()
            .filter(|implementation| implementation.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        assert_eq!(implementations.len(), 2);

        for implementation in implementations {
            assert!(implementation.generic_type_parameters().is_empty());
            assert!(implementation.generic_const_parameters().is_empty());
        }
    }

    #[test]
    fn with_clauses_do_not_introduce_implementation_parameters() {
        let graph = graph(&[concat!(
            "module app;\n",
            "struct Buffer<T>\n",
            "{\n",
            "}\n",
            "impl Buffer<i32> with(T: i32)\n",
            "{\n",
            "}\n",
        )]);

        let implementation = source_implementation(&graph);

        assert!(implementation.generic_type_parameters().is_empty());
        assert!(implementation.generic_const_parameters().is_empty());
    }

    #[test]
    fn repeated_and_concurrent_construction_is_deterministic() {
        let source = Arc::<str>::from(concat!(
            "module app;\n",
            "struct Buffer<T>\n",
            "{\n",
            "}\n",
            "impl Buffer<T>\n",
            "{\n",
            "}\n",
        ));
        let expected = graph(&[source.as_ref()]);
        let mut workers = Vec::new();

        for _ in 0..4 {
            let source = Arc::clone(&source);

            workers.push(std::thread::spawn(move || graph(&[source.as_ref()])));
        }

        for worker in workers {
            let actual = match worker.join() {
                Ok(graph) => graph,
                Err(_) => panic!("symbol construction worker should not panic"),
            };

            assert_eq!(actual, expected);
        }
    }

    fn graph(source_texts: &[&str]) -> SymbolGraph {
        let (declarations, syntax) = crate::test_support::declarations_and_syntax(source_texts);

        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity should be valid");
        };

        match SymbolGraph::build_source(package, &declarations, &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        }
    }

    fn source_implementation(graph: &SymbolGraph) -> &crate::InherentImplementationSymbol {
        let implementations = graph
            .inherent_implementations()
            .iter()
            .filter(|implementation| implementation.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [implementation] = implementations.as_slice() else {
            panic!("test source should declare one inherent implementation");
        };

        implementation
    }

    fn source_unnamed_trait_implementation(
        graph: &SymbolGraph,
    ) -> &crate::UnnamedTraitImplementationSymbol {
        let implementations = graph
            .unnamed_trait_implementations()
            .iter()
            .filter(|implementation| implementation.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [implementation] = implementations.as_slice() else {
            panic!("test source should declare one unnamed trait implementation");
        };

        implementation
    }
}
