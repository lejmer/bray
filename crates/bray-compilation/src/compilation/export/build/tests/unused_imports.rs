use bray_diagnostics::{DiagnosticKind, SeverityKind};
use bray_messages::DiagnosticRenderer;

use super::fixtures::{compilation, compilation_from_sources, execution_consumer};
use crate::Compilation;
use crate::fact::{CancellationToken, QueryPriority};

const API: &str = r#"
module api
{
    struct Value
    {
        number: i32;
    }

    func read() -> i32
    {
        return 1;
    }

    const func positive(pos value: i32) -> bool
    {
        return value > 0;
    }
}

module api.child
{
    func read() -> i32
    {
        return 2;
    }
}

module api2
{
    func read() -> i32
    {
        return 3;
    }
}
"#;

fn unused_imports(compilation: &Compilation) -> Vec<String> {
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingUnusedUsing)
        .map(|diagnostic| {
            DiagnosticRenderer::english()
                .render(diagnostic)
                .message()
                .to_owned()
        })
        .collect()
}

#[test]
fn unused_imports_warn_at_the_validated_declaration() {
    let source = r#"
module app;

using api;

func run() {}
"#;

    let compilation = compilation_from_sources([API, source]);
    let diagnostics = compilation.check_diagnostics();

    bray_testing::assert_goal_state_diagnostic_kind(
        diagnostics,
        DiagnosticKind::BindingUnusedUsing,
    );

    assert_eq!(diagnostics.len(), 1);

    let diagnostic = diagnostics
        .iter()
        .next()
        .expect("unused using produces a warning");

    let span = diagnostic
        .primary_span()
        .expect("warning locates its using declaration");

    let snapshot = compilation
        .source(span.source_id())
        .expect("warning source exists");

    let rendered = DiagnosticRenderer::english().render(diagnostic);

    assert_eq!(
        snapshot
            .text_slice(span.range())
            .expect("span belongs to source")
            .trim(),
        "using api;"
    );

    assert_eq!(diagnostic.severity(), SeverityKind::Warning);
    assert_eq!(rendered.labels()[0].span(), span);
}

#[test]
fn declaration_paths_count_as_source_and_imported_dependency_usage() {
    let provider = compilation(API);

    for imported in [false, true] {
        let prefix = if imported { "example.package." } else { "" };

        for reference in [
            r#"
func run(pos value: &PREFIXapi.Value) {}
"#,
            r#"
struct Holder
{
    value: PREFIXapi.Value;
}
"#,
            r#"
func run(pos value: i32)
    requires(PREFIXapi.positive(value)) {}
"#,
            r#"
func run(pos value: i32 = PREFIXapi.read()) {}
"#,
            r#"
export PREFIXapi.Value;
"#,
        ] {
            let source = format!(
                r#"
module app;

using {prefix}api;

{}
"#,
                reference.replace("PREFIX", prefix)
            );

            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([API, &source])
            };

            assert!(unused_imports(&consumer).is_empty(), "{source}");
        }
    }
}

#[test]
fn unused_imported_dependencies_warn() {
    let provider = compilation(API);

    let consumer = execution_consumer(
        &provider,
        r#"
module app;

using example.package.api;
"#,
    );

    assert_eq!(
        unused_imports(&consumer),
        ["using declaration for 'example.package.api' is unused"]
    );
}

#[test]
fn local_declaration_references_count_as_usage() {
    let consumer = compilation(
        r#"
module app;

using Local;

struct Local
{
    number: i32;
}

func run(pos value: &Local) {}
"#,
    );

    assert!(unused_imports(&consumer).is_empty());
}

#[test]
fn enabled_split_contributions_share_import_usage() {
    for (contribution, count) in [
        (
            r#"
module app;

func run() -> i32
{
    return api.read();
}
"#,
            0,
        ),
        (
            r#"
@target(false)
module app;

func run() -> i32
{
    return api.read();
}
"#,
            1,
        ),
    ] {
        let consumer = compilation_from_sources([
            API,
            r#"
module app;

using api;
"#,
            contribution,
        ]);

        assert_eq!(unused_imports(&consumer).len(), count, "{contribution}");
    }

    let consumer = compilation_from_sources([
        API,
        r#"
module app;

using api;
using api.read;
"#,
        r#"
module app;

using api;

func run() -> i32
{
    return api.read();
}
"#,
    ]);

    assert!(unused_imports(&consumer).is_empty());
}

#[test]
fn independent_modules_do_not_share_import_usage() {
    let unused = r#"
module app;

using api;
"#;

    let consumer = compilation_from_sources([
        API,
        unused,
        r#"
module other;

using api;

func run() -> i32
{
    return api.read();
}
"#,
    ]);

    assert_eq!(
        unused_imports(&consumer),
        ["using declaration for 'api' is unused"]
    );

    let source = consumer.sources().iter().find(|source| source.text() == unused)
        .expect("the independent app contribution remains available");

    assert!(consumer.check_diagnostics().by_kind(DiagnosticKind::BindingUnusedUsing).any(|diagnostic|
        crate::test_support::diagnostic_at_call(diagnostic, source.source_id(), unused, "using api")
    ));
}

#[test]
fn import_usage_requires_complete_path_components() {
    let consumer = compilation_from_sources([
        API,
        r#"
module app;

using api;
using api2;

func run() -> i32
{
    return api2.read();
}
"#,
    ]);

    assert_eq!(
        unused_imports(&consumer),
        ["using declaration for 'api' is unused"]
    );
}

#[test]
fn using_targets_do_not_count_as_dependency_usage() {
    let consumer = compilation_from_sources([
        API,
        r#"
module app;

using api;
using api.read;
"#,
    ]);

    assert_eq!(
        unused_imports(&consumer),
        [
            "using declaration for 'api' is unused",
            "using declaration for 'api.read' is unused",
        ]
    );
}

#[test]
fn namespace_usage_survives_re_exports_and_internal_acknowledgements() {
    let provider = compilation(
        r#"
internal module details
{
    internal struct Value
    {
        number: i32;
    }
}

module facade
{
    using internal details;
    export details.Value;
}
"#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
module app;

using internal example.package.facade;

func run(pos value: &example.package.facade.Value) {}
"#,
    );

    assert!(unused_imports(&consumer).is_empty());

    let unused = execution_consumer(
        &provider,
        r#"
module app;

using internal example.package.facade;
"#,
    );

    assert_eq!(
        unused_imports(&unused),
        ["using declaration for 'example.package.facade' is unused"]
    );
}

#[test]
fn alternative_pattern_bindings_preserve_each_internal_field_acknowledgement() {
    let provider = compilation(
        r#"
module api;

struct First
{
    internal data: i32;
}

struct Second
{
    internal data: i32;
}

union Choice
{
    One(pos value: First);
    Two(pos value: Second);
}
"#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
module app;

using example.package.api;
using internal example.package.api.First.data;
using internal example.package.api.Second.data;

func run(pos input: example.package.api.Choice)
{
    match input
    {
        case One(value = { data }) | Two(value = { data })
        {
            _ = data;
        }
    }
}
"#,
    );

    assert!(unused_imports(&consumer).is_empty());
}

#[test]
fn qualified_return_types_keep_module_lookup_when_generic_names_overlap() {
    let consumer = compilation_from_sources([
        API,
        r#"
module app;

using api;

func create<api>() -> api.Value
{
    return
    {
        number = 0
    };
}
"#,
    ]);

    assert!(unused_imports(&consumer).is_empty());
}

#[test]
fn local_values_do_not_count_as_namespace_uses() {
    let provider = compilation(API);

    let consumer = execution_consumer(
        &provider,
        r#"
module app;

using example.package;

struct Data
{
    number: i32;
}

struct Local
{
    package: Data;
}

func run(pos example: &Local) -> i32
{
    return example.package.number;
}
"#,
    );

    assert_eq!(
        unused_imports(&consumer),
        ["using declaration for 'example.package' is unused"]
    );
}

#[test]
fn unresolved_imports_keep_their_original_error_without_unused_warnings() {
    let source = r#"
module app;

using missing;
"#;

    let consumer = compilation(source);
    let diagnostics = consumer.check_diagnostics();

    let source_id = consumer.sources().iter().next()
        .expect("the unresolved using source remains available")
        .source_id();

    assert!(diagnostics.by_kind(DiagnosticKind::BindingUnresolvedName).any(|diagnostic|
        crate::test_support::diagnostic_at_call(diagnostic, source_id, source, "missing")
    ), "{diagnostics:?}");

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingUnusedUsing)
    );
}

#[test]
fn signature_type_parameters_do_not_count_as_namespace_uses() {
    let consumer = compilation_from_sources([
        API,
        r#"
module app;

using api;

func run<api>(pos value: api) {}
"#,
    ]);

    assert_eq!(
        unused_imports(&consumer),
        ["using declaration for 'api' is unused"]
    );
}

#[test]
fn usage_is_independent_of_reference_queries_worker_count_and_source_order() {
    let first = r#"
module app;

using api;
using api2;
"#;

    let second = r#"
module app;

func run(pos value: &api.Value) {}
"#;

    for sources in [[API, first, second], [second, first, API]] {
        for workers in [1, 2] {
            for demand_references in [false, true] {
                let consumer = crate::test_support::compilation_with_sources_and_worker_budget(
                &sources,
                crate::WorkerBudget::new(workers).expect("positive worker budget"),
            );

                if demand_references {
                    let declaration = consumer.sources().iter().find(|source| source.text() == API)
                        .expect("the referenced type's source is available");

                    let offset = API.find("struct Value").expect("the fixture declares Value") + "struct ".len();

                    let position = bray_source::TextSize::new(
                        u32::try_from(offset).expect("fixture offset fits source"),
                    );

                    let use_source = consumer.sources().iter().find(|source| source.text() == second)
                        .expect("the referencing source is available");

                    let references = consumer
                        .references_at(
                            declaration.source_id(),
                            position,
                            &CancellationToken::new(),
                            QueryPriority::Interactive,
                        )
                        .expect("public references query succeeds");

                    let crate::SemanticAvailability::Available(references) = references else {
                        panic!("the fixture must resolve references to Value");
                    };

                    assert!(references.iter().any(|reference|
                        reference.syntax().source_id() == use_source.source_id()
                    ));
                }

                assert_eq!(
                    unused_imports(&consumer),
                    ["using declaration for 'api2' is unused"]
                );
            }
        }
    }
}
