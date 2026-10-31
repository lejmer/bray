use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bray_base::Cancellation;
use bray_diagnostics::{DiagnosticIoErrorKind, DiagnosticProjectGeneratorFailure as Failure};
use bray_project::{
    GeneratorCache, GeneratorExecutable, GeneratorHost, GeneratorInputIdentity, GeneratorOutput,
    GeneratorRequest, GeneratorTool, ProjectBuildNode, ProjectGraph, ProjectLoadError,
    SandboxedGeneratorExecutor, canonicalize_package_manifest, load_project_graph,
    load_project_graph_with_generators,
};
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::NativeTarget;
use serde_json::{Value, json};

struct Fixture {
    directory: tempfile::TempDir,
    workspace: Value,
    package: Value,
    tool: Value,
}

impl Fixture {
    fn new() -> Self {
        let host = NativeTarget::current().expect("generator tests require a supported host");
        let directory = tempfile::tempdir().expect("test workspace must be available");

        let fixture = Self {
            directory,
            workspace: json!({
                "format": 1, "output_root": "build",
                "targets": [{"name": "native", "identity": host.as_str()}],
                "packages": [{"path": "app", "role": "root"}, {"path": "tool", "role": "vendored"}],
            }),
            package: json!({
                "format": 1, "identity": "test.app", "version": "1.0.0", "source_roots": [],
                "generators": [generator("bindings")],
                "products": [{"name": "app", "kind": "library", "source_roots": [], "generated_sources": ["bindings"], "targets": ["native"], "outputs": ["package_interface"]}],
            }),
            tool: json!({
                "format": 1, "identity": "test.tool", "version": "1.0.0", "source_roots": [{"name": "main", "path": "src"}],
                "products": [{"name": "generator", "kind": "executable", "source_roots": ["main"], "targets": ["native"], "outputs": ["executable"]}],
            }),
        };

        fs::create_dir_all(fixture.root().join("tool/src"))
            .expect("tool source directory must be writable");

        fs::create_dir_all(fixture.root().join("app")).expect("app directory must be writable");

        fs::write(
            fixture.root().join("tool/src/main.bray"),
            r#"
module tool;

func main() -> i32
{
    return 0;
}
"#,
        )
        .expect("tool source must be writable");

        fs::write(fixture.root().join("app/schema.txt"), "schema").expect("input must be writable");
        fixture.save();

        fixture
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn save(&self) {
        for (path, value) in [
            ("bray-workspace.json", &self.workspace),
            ("app/bray-package.json", &self.package),
            ("tool/bray-package.json", &self.tool),
        ] {
            fs::write(
                self.root().join(path),
                serde_json::to_vec_pretty(value).expect("test manifests must serialize"),
            )
            .expect("manifest must be writable");
        }
    }

    fn load(
        &self,
        cache: &mut GeneratorCache,
        executor: &dyn SandboxedGeneratorExecutor,
    ) -> Result<ProjectGraph, ProjectLoadError> {
        self.load_with(cache, executor, b"tool", [1; 32], [2; 32], &|| false)
    }

    fn load_with(
        &self,
        cache: &mut GeneratorCache,
        executor: &dyn SandboxedGeneratorExecutor,
        artifact: &[u8],
        toolchain_identity: [u8; 32],
        policy_identity: [u8; 32],
        cancellation: &dyn Cancellation,
    ) -> Result<ProjectGraph, ProjectLoadError> {
        let tool = GeneratorTool::new(
            ProductIdentity::try_new(
                PackageIdentity::try_new("test.tool").expect("valid package"),
                "generator",
            )
            .expect("valid product"),
            NativeTarget::current().expect("supported host").identity(),
        );

        let executables = BTreeMap::from([(
            tool,
            GeneratorExecutable::new(artifact.into(), toolchain_identity),
        )]);

        load_project_graph_with_generators(
            self.root(),
            &mut GeneratorHost {
                executables: &executables,
                executor,
                cache,
                cache_mode: bray_project::GeneratorCacheMode::Reuse,
                policy_identity,
                cancellation,
            },
        )
    }
}

fn generator(name: &str) -> Value {
    json!({
        "name": name, "tool": {"package": "test.tool", "product": "generator", "target": "native"},
        "inputs": ["schema.txt"], "generated_inputs": [], "outputs": [format!("{name}.bray")],
        "environment": {"MODE": "stable"}, "permissions": {"network": false},
    })
}

struct Executor<F> {
    calls: AtomicUsize,
    run: F,
}

impl<F> Executor<F> {
    fn new(run: F) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            run,
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl<F> SandboxedGeneratorExecutor for Executor<F>
where
    F: Fn(&GeneratorRequest<'_>) -> Result<Vec<GeneratorOutput>, Failure> + Send + Sync,
{
    fn execute(
        &self,
        request: &GeneratorRequest<'_>,
        _: &dyn Cancellation,
    ) -> Result<Vec<GeneratorOutput>, Failure> {
        self.calls.fetch_add(1, Ordering::Relaxed);

        (self.run)(request)
    }
}

fn outputs(request: &GeneratorRequest<'_>) -> Result<Vec<GeneratorOutput>, Failure> {
    Ok(request
        .generator()
        .outputs()
        .iter()
        .map(|path| GeneratorOutput {
            path: path.as_str().into(),
            bytes: b"module generated;\n".to_vec(),
        })
        .collect())
}

fn failure(result: Result<ProjectGraph, ProjectLoadError>) -> Failure {
    match result {
        Err(ProjectLoadError::Generator { failure, .. }) => failure,
        result => panic!("expected exact generator failure, received {result:?}"),
    }
}

#[test]
fn generated_only_products_publish_distinct_immutable_sources_and_host_order() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProjectGraph>();

    let fixture = Fixture::new();

    let executor = Executor::new(|request: &GeneratorRequest<'_>| {
        assert_eq!(request.executable().bytes(), b"tool");

        assert!(
            [b"schema".as_slice(), b"changed".as_slice()].contains(&request.inputs()[0].bytes())
        );

        assert_eq!(
            request
                .generator()
                .environment()
                .get("MODE")
                .map(AsRef::as_ref),
            Some("stable")
        );

        assert_eq!(
            request.inputs()[0].identity(),
            &GeneratorInputIdentity::Source(
                bray_project::ProjectPath::try_relative("schema.txt").expect("valid path")
            )
        );

        outputs(request)
    });

    let mut cache = GeneratorCache::default();

    let graph = fixture
        .load(&mut cache, &executor)
        .expect("generator graph must load");

    let source = &graph.generated_sources()[0];

    assert_eq!(source.generator().name(), "bindings");
    assert_eq!(source.generator().package().as_str(), "test.app");
    assert_eq!(source.output().as_str(), "bindings.bray");
    assert_eq!(source.text(), "module generated;\n");
    assert!(graph.packages()[0].products()[0].sources().is_empty());
    assert!(!fixture.root().join("app/bindings.bray").exists());

    assert!(
        matches!(graph.build_order(), [ProjectBuildNode::Product { product: tool, .. }, ProjectBuildNode::Generator { .. }, ProjectBuildNode::Product { product: app, .. }] if tool.package().as_str() == "test.tool" && app.package().as_str() == "test.app")
    );

    fs::write(fixture.root().join("app/schema.txt"), "changed").expect("input must be writable");

    let changed = fixture
        .load(&mut cache, &executor)
        .expect("changed graph must load");

    assert_ne!(
        source.cache_key(),
        changed.generated_sources()[0].cache_key()
    );

    assert_eq!(source.text(), "module generated;\n");
    assert_eq!(executor.calls(), 2);
}

#[test]
fn loading_without_an_executor_fails_closed() {
    let fixture = Fixture::new();

    assert_eq!(
        failure(load_project_graph(fixture.root())),
        Failure::ExecutorUnavailable
    );
}

#[test]
fn generated_dependencies_feed_snapshots_to_consumers_in_order() {
    let mut fixture = Fixture::new();
    let mut consumer = generator("consumer");

    consumer["generated_inputs"] = json!([{"generator": "bindings", "output": "bindings.bray"}]);

    fixture.package["generators"]
        .as_array_mut()
        .expect("array")
        .push(consumer);

    fixture.package["products"][0]["generated_sources"] = json!(["consumer"]);
    fixture.save();

    let executor = Executor::new(|request: &GeneratorRequest<'_>| {
        if request.generator().identity().name() == "consumer" {
            assert_eq!(request.inputs().len(), 2);
            assert_eq!(request.inputs()[1].bytes(), b"module generated;\n");

            assert!(
                matches!(request.inputs()[1].identity(), GeneratorInputIdentity::Generated { generator, .. } if generator.name() == "bindings")
            );
        }

        outputs(request)
    });

    let graph = fixture
        .load(&mut GeneratorCache::default(), &executor)
        .expect("generated chain must load");

    assert_eq!(
        graph
            .generated_sources()
            .iter()
            .map(|source| source.generator().name())
            .collect::<Vec<_>>(),
        ["bindings", "consumer"]
    );
}

#[test]
fn generated_and_mixed_host_tool_cycles_fail_before_execution() {
    for mixed in [false, true] {
        let mut fixture = Fixture::new();

        if mixed {
            fixture.tool["products"][0]["dependencies"] =
                json!([{"package": "test.app", "product": "app"}]);
        } else {
            fixture.package["generators"][0]["generated_inputs"] =
                json!([{"generator": "bindings", "output": "bindings.bray"}]);
        }

        fixture.save();

        let executor = Executor::new(outputs);

        assert_eq!(
            failure(fixture.load(&mut GeneratorCache::default(), &executor)),
            Failure::DependencyCycle
        );

        assert_eq!(executor.calls(), 0);
    }
}

#[test]
fn output_inventory_is_validated_before_publication() {
    for (paths, expected) in [
        (
            vec![],
            Failure::MissingOutput {
                path: "bindings.bray".into(),
            },
        ),
        (
            vec!["extra.bray"],
            Failure::UndeclaredOutput {
                path: "extra.bray".into(),
            },
        ),
        (
            vec!["../bindings.bray"],
            Failure::InvalidOutput {
                path: "../bindings.bray".into(),
            },
        ),
        (
            vec!["bindings.bray", "bindings.bray"],
            Failure::OutputCollision {
                path: "bindings.bray".into(),
            },
        ),
    ] {
        let fixture = Fixture::new();

        let executor = Executor::new(|_: &GeneratorRequest<'_>| {
            Ok(paths
                .iter()
                .map(|path| GeneratorOutput {
                    path: (*path).into(),
                    bytes: b"module generated;\n".to_vec(),
                })
                .collect())
        });

        let mut cache = GeneratorCache::default();

        assert_eq!(failure(fixture.load(&mut cache, &executor)), expected);

        let successful = Executor::new(outputs);

        fixture
            .load(&mut cache, &successful)
            .expect("failed output cannot enter the cache");

        assert_eq!(successful.calls(), 1);
    }

    let fixture = Fixture::new();

    let executor = Executor::new(|_: &GeneratorRequest<'_>| {
        Ok(vec![GeneratorOutput {
            path: "bindings.bray".into(),
            bytes: vec![255],
        }])
    });

    assert_eq!(
        failure(fixture.load(&mut GeneratorCache::default(), &executor)),
        Failure::NonUtf8Output {
            path: "bindings.bray".into()
        }
    );
}

#[test]
fn every_execution_identity_invalidates_cached_results() {
    let mut fixture = Fixture::new();
    let executor = Executor::new(outputs);
    let mut cache = GeneratorCache::default();

    let initial = fixture
        .load(&mut cache, &executor)
        .expect("initial generation");

    assert_eq!(
        fixture
            .load(&mut cache, &executor)
            .expect("cached generation"),
        initial
    );

    assert_eq!(executor.calls(), 1);

    for (artifact, toolchain, policy) in [
        (b"other".as_slice(), [1; 32], [2; 32]),
        (b"tool".as_slice(), [3; 32], [2; 32]),
        (b"tool".as_slice(), [1; 32], [4; 32]),
    ] {
        let graph = fixture
            .load_with(&mut cache, &executor, artifact, toolchain, policy, &|| {
                false
            })
            .expect("identity invalidation");

        assert_ne!(
            graph.generated_sources()[0].cache_key(),
            initial.generated_sources()[0].cache_key()
        );
    }

    fixture.package["generators"][0]["environment"]["MODE"] = json!("changed");
    fixture.save();

    let environment = fixture
        .load(&mut cache, &executor)
        .expect("environment invalidation");

    assert_ne!(
        environment.generated_sources()[0].cache_key(),
        initial.generated_sources()[0].cache_key()
    );

    assert_eq!(executor.calls(), 5);
}

#[test]
fn target_observations_are_explicit_and_target_specific() {
    let mut fixture = Fixture::new();

    let other = if NativeTarget::current().expect("host").as_str() == "aarch64-pc-windows-msvc" {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-pc-windows-msvc"
    };

    fixture.workspace["targets"]
        .as_array_mut()
        .expect("array")
        .push(json!({"name": "portable", "identity": other}));

    fixture.package["products"][0]["targets"] = json!(["native", "portable"]);
    fixture.save();

    let executor = Executor::new(outputs);

    let graph = fixture
        .load(&mut GeneratorCache::default(), &executor)
        .expect("cross-target graph");

    let sources = graph.generated_sources();

    assert_eq!(sources.len(), 2);
    assert_ne!(sources[0].target(), sources[1].target());
    assert_ne!(sources[0].cache_key(), sources[1].cache_key());
    assert_eq!(executor.calls(), 2);
}

#[test]
fn failure_or_cancellation_cannot_publish_partial_graphs_or_cache_entries() {
    for cancel in [false, true] {
        let mut fixture = Fixture::new();

        fixture.package["generators"]
            .as_array_mut()
            .expect("array")
            .push(generator("second"));

        fixture.package["products"][0]["generated_sources"] = json!(["bindings", "second"]);
        fixture.save();

        let cancelled = AtomicBool::new(false);

        let executor = Executor::new(|request: &GeneratorRequest<'_>| {
            if request.generator().identity().name() == "second" {
                if !cancel {
                    return Err(Failure::ExecutionFailed { code: Some(7) });
                }

                cancelled.store(true, Ordering::Release);
            }

            outputs(request)
        });

        let mut cache = GeneratorCache::default();

        let result = fixture.load_with(&mut cache, &executor, b"tool", [1; 32], [2; 32], &|| {
            cancelled.load(Ordering::Acquire)
        });

        assert_eq!(
            failure(result),
            if cancel {
                Failure::Cancelled
            } else {
                Failure::ExecutionFailed { code: Some(7) }
            }
        );

        assert_eq!(executor.calls(), 2);

        let successful = Executor::new(outputs);

        fixture
            .load(&mut cache, &successful)
            .expect("retry must succeed");

        assert_eq!(successful.calls(), 2);
    }
}

#[test]
fn generator_permissions_tool_selection_and_environment_are_closed() {
    for (field, value, expected) in [
        (
            "environment",
            json!({"BAD=NAME": "value"}),
            Failure::InvalidEnvironment {
                name: "BAD=NAME".into(),
            },
        ),
        (
            "outputs",
            json!(["file.bray", "file.bray/nested.bray"]),
            Failure::OutputCollision {
                path: "file.bray".into(),
            },
        ),
        (
            "outputs",
            json!(["out.txt"]),
            Failure::InvalidOutput {
                path: "out.txt".into(),
            },
        ),
        (
            "generated_inputs",
            json!([{"generator": "absent", "output": "out.bray"}]),
            Failure::UnknownReference {
                name: "test.app/absent/out.bray".into(),
            },
        ),
    ] {
        let mut fixture = Fixture::new();

        fixture.package["generators"][0][field] = value;
        fixture.save();

        let executor = Executor::new(outputs);

        assert_eq!(
            failure(fixture.load(&mut GeneratorCache::default(), &executor)),
            expected
        );

        assert_eq!(executor.calls(), 0);
    }
}

#[test]
fn source_input_io_preserves_its_exact_cause() {
    let mut fixture = Fixture::new();

    fixture.package["generators"][0]["inputs"] = json!(["absent"]);
    fixture.save();

    assert_eq!(
        failure(fixture.load(&mut GeneratorCache::default(), &Executor::new(outputs))),
        Failure::InputIo {
            path: "absent".into(),
            kind: DiagnosticIoErrorKind::NotFound
        }
    );
}

#[cfg(unix)]
#[test]
fn generator_inputs_cannot_follow_symlinks_even_in_parent_components() {
    let fixture = Fixture::new();

    std::os::unix::fs::symlink(
        fixture.root().join("tool"),
        fixture.root().join("app/linked"),
    )
    .expect("symlink must be creatable");

    let mut fixture = fixture;

    fixture.package["generators"][0]["inputs"] = json!(["linked/src/main.bray"]);
    fixture.save();

    assert_eq!(
        failure(fixture.load(&mut GeneratorCache::default(), &Executor::new(outputs))),
        Failure::InputNotRegular {
            path: "linked/src/main.bray".into()
        }
    );
}

#[test]
fn canonicalization_normalizes_sets_but_preserves_input_order() {
    let fixture = Fixture::new();
    let mut manifest = fixture.package;

    manifest["generators"][0]["inputs"] = json!(["z.txt", "a.txt"]);
    manifest["generators"][0]["outputs"] = json!(["z.bray", "a.bray"]);

    let source =
        canonicalize_package_manifest(&manifest.to_string(), Path::new("bray-package.json"))
            .expect("manifest serialization");

    let decoded: Value = serde_json::from_str(&source).expect("canonical JSON");

    assert_eq!(
        decoded["generators"][0]["inputs"],
        json!(["z.txt", "a.txt"])
    );

    assert_eq!(
        decoded["generators"][0]["outputs"],
        json!(["a.bray", "z.bray"])
    );

    manifest["generators"][0]["permissions"]["filesystem"] = json!(true);

    assert!(
        canonicalize_package_manifest(&manifest.to_string(), Path::new("bray-package.json"))
            .is_err()
    );
}

#[test]
fn reproducibility_verification_catches_changed_outputs_for_equal_inputs() {
    let fixture = Fixture::new();
    let mut cache = GeneratorCache::default();

    fixture
        .load(&mut cache, &Executor::new(outputs))
        .expect("initial generation");

    let executor = Executor::new(|_: &GeneratorRequest<'_>| {
        Ok(vec![GeneratorOutput {
            path: "bindings.bray".into(),
            bytes: b"module different;\n".to_vec(),
        }])
    });

    let tool = GeneratorTool::new(
        ProductIdentity::try_new(
            PackageIdentity::try_new("test.tool").expect("valid package"),
            "generator",
        )
        .expect("valid product"),
        NativeTarget::current().expect("supported host").identity(),
    );

    let executables = BTreeMap::from([(
        tool,
        GeneratorExecutable::new(Arc::from(b"tool".as_slice()), [1; 32]),
    )]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        load_project_graph_with_generators(
            fixture.root(),
            &mut GeneratorHost {
                executables: &executables,
                executor: &executor,
                cache: &mut cache,
                cache_mode: bray_project::GeneratorCacheMode::Verify,
                policy_identity: [2; 32],
                cancellation: &|| false,
            },
        )
    }));

    assert!(
        result.is_err(),
        "nondeterministic output is a generator invariant failure"
    );

    assert_eq!(executor.calls(), 1);
}

#[test]
fn explicit_network_grants_reach_the_executor_and_invalidate_cache_identity() {
    let mut fixture = Fixture::new();
    let mut cache = GeneratorCache::default();

    let denied = Executor::new(|request: &GeneratorRequest<'_>| {
        assert!(!request.generator().permissions().network());

        outputs(request)
    });

    let initial = fixture
        .load(&mut cache, &denied)
        .expect("denied network graph");

    fixture.package["generators"][0]["permissions"]["network"] = json!(true);
    fixture.save();

    let granted = Executor::new(|request: &GeneratorRequest<'_>| {
        assert!(request.generator().permissions().network());

        outputs(request)
    });

    let graph = fixture
        .load(&mut cache, &granted)
        .expect("explicit network graph");

    assert_ne!(
        initial.generated_sources()[0].cache_key(),
        graph.generated_sources()[0].cache_key()
    );

    assert_eq!(granted.calls(), 1);
}

#[test]
fn sandbox_rejections_are_preserved_without_publication() {
    let fixture = Fixture::new();

    for expected in [
        Failure::SandboxUnavailable,
        Failure::AccessDenied {
            resource: "network".into(),
        },
        Failure::ExecutorIo {
            kind: DiagnosticIoErrorKind::PermissionDenied,
        },
    ] {
        let executor = Executor::new(|_: &GeneratorRequest<'_>| Err(expected.clone()));

        assert_eq!(
            failure(fixture.load(&mut GeneratorCache::default(), &executor)),
            expected
        );
    }
}

#[test]
fn products_select_only_their_declared_generated_outputs() {
    let mut fixture = Fixture::new();
    let mut consumer = generator("consumer");

    consumer["generated_inputs"] = json!([{"generator": "bindings", "output": "bindings.bray"}]);

    fixture.package["generators"]
        .as_array_mut()
        .expect("array")
        .push(consumer);

    fixture.package["products"][0]["generated_sources"] = json!(["consumer"]);
    fixture.save();

    let graph = fixture
        .load(&mut GeneratorCache::default(), &Executor::new(outputs))
        .expect("graph");

    let product = &graph.packages()[0].products()[0];

    assert_eq!(
        graph
            .product_generated_sources(product, &NativeTarget::current().expect("host").identity())
            .map(|source| source.generator().name())
            .collect::<Vec<_>>(),
        ["consumer"]
    );
}

#[test]
fn manifest_order_and_input_order_have_distinct_identity_contracts() {
    let mut fixture = Fixture::new();

    fixture.package["generators"][0]["outputs"] = json!(["b.bray", "a.bray"]);
    fixture.package["generators"][0]["inputs"] = json!(["schema.txt", "second.txt"]);
    fs::write(fixture.root().join("app/second.txt"), "second").expect("input");
    fixture.save();

    let mut cache = GeneratorCache::default();
    let executor = Executor::new(outputs);
    let initial = fixture.load(&mut cache, &executor).expect("initial graph");

    fixture.package["generators"][0]["outputs"] = json!(["a.bray", "b.bray"]);

    fixture.workspace["packages"]
        .as_array_mut()
        .expect("array")
        .reverse();

    fixture.save();

    assert_eq!(
        initial,
        fixture.load(&mut cache, &executor).expect("reordered sets")
    );

    assert_eq!(executor.calls(), 1);

    fixture.package["generators"][0]["inputs"] = json!(["second.txt", "schema.txt"]);
    fixture.save();

    let reordered = fixture.load(&mut cache, &executor).expect("ordered inputs");

    assert_ne!(
        initial.generated_sources()[0].cache_key(),
        reordered.generated_sources()[0].cache_key()
    );
}

#[test]
fn standard_library_generators_use_the_same_sandbox_boundary() {
    let mut fixture = Fixture::new();

    fixture.package["identity"] = json!("std");
    fixture.tool["identity"] = json!("std.tool");
    fixture.package["generators"][0]["tool"]["package"] = json!("std.tool");
    fixture.save();

    let tool = GeneratorTool::new(
        ProductIdentity::try_new(
            PackageIdentity::try_new("std.tool").expect("valid package"),
            "generator",
        )
        .expect("valid product"),
        NativeTarget::current().expect("supported host").identity(),
    );

    let executables = BTreeMap::from([(
        tool,
        GeneratorExecutable::new(Arc::from(b"tool".as_slice()), [1; 32]),
    )]);

    let executor = Executor::new(outputs);
    let mut cache = GeneratorCache::default();

    let graph = bray_project::load_standard_library_project_graph_with_generators(
        fixture.root(),
        &mut GeneratorHost {
            executables: &executables,
            executor: &executor,
            cache: &mut cache,
            cache_mode: bray_project::GeneratorCacheMode::Reuse,
            policy_identity: [2; 32],
            cancellation: &|| false,
        },
    )
    .expect("standard library generation");

    assert!(graph.source_authority().is_standard_library());

    assert_eq!(
        graph.generated_sources()[0].generator().package().as_str(),
        "std"
    );
}

#[test]
fn unused_generators_do_not_read_inputs_or_require_an_executor() {
    let mut fixture = Fixture::new();

    fs::create_dir(fixture.root().join("app/src")).expect("source directory");

    fs::write(
        fixture.root().join("app/src/main.bray"),
        "module application;\n",
    )
    .expect("source file");

    fixture.package["source_roots"] = json!([{"name": "main", "path": "src"}]);
    fixture.package["products"][0]["source_roots"] = json!(["main"]);
    fixture.package["products"][0]["generated_sources"] = json!([]);
    fixture.package["generators"][0]["inputs"] = json!(["absent"]);
    fixture.save();

    let graph =
        load_project_graph(fixture.root()).expect("unused generators need no execution resources");

    assert!(graph.generated_sources().is_empty());
    assert_eq!(graph.packages()[0].products()[0].sources().len(), 1);
}

#[test]
fn cancellation_is_observed_before_reading_inputs() {
    let mut fixture = Fixture::new();

    fixture.package["generators"][0]["inputs"] = json!(["absent"]);
    fixture.save();

    let executor = Executor::new(outputs);

    let result = fixture.load_with(
        &mut GeneratorCache::default(),
        &executor,
        b"tool",
        [1; 32],
        [2; 32],
        &|| true,
    );

    assert_eq!(failure(result), Failure::Cancelled);
    assert_eq!(executor.calls(), 0);
}

#[test]
fn tools_must_be_exact_executable_products_for_the_execution_host() {
    let mut fixture = Fixture::new();

    fixture.tool["products"][0]["kind"] = json!("library");
    fixture.save();

    assert!(
        matches!(failure(fixture.load(&mut GeneratorCache::default(), &Executor::new(outputs))), Failure::ToolNotExecutable { tool } if tool.starts_with("test.tool/generator/"))
    );

    fixture.tool["products"][0]["kind"] = json!("executable");

    let other = if NativeTarget::current().expect("host").as_str() == "aarch64-pc-windows-msvc" {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-pc-windows-msvc"
    };

    fixture.workspace["targets"]
        .as_array_mut()
        .expect("array")
        .push(json!({"name": "portable", "identity": other}));

    fixture.tool["products"][0]["targets"] = json!(["portable"]);
    fixture.package["generators"][0]["tool"]["target"] = json!("portable");
    fixture.save();

    assert_eq!(
        failure(fixture.load(&mut GeneratorCache::default(), &Executor::new(outputs))),
        Failure::ToolTargetMismatch {
            target: other.into(),
            host_target: Some(
                NativeTarget::current()
                    .expect("supported host")
                    .as_str()
                    .into()
            ),
        }
    );
}

#[cfg(unix)]
#[test]
fn ordinary_source_roots_cannot_traverse_symlinked_parent_directories() {
    let mut fixture = Fixture::new();

    std::os::unix::fs::symlink(
        fixture.root().join("tool"),
        fixture.root().join("app/linked"),
    )
    .expect("symlink");

    fixture.package["source_roots"] = json!([{"name": "main", "path": "linked/src"}]);
    fixture.package["products"][0]["source_roots"] = json!(["main"]);
    fixture.save();

    assert!(
        matches!(load_project_graph(fixture.root()), Err(ProjectLoadError::InvalidManifest { problem, .. }) if problem.kind() == bray_project::ProjectManifestProblemKind::SourceSymlink)
    );
}

#[test]
fn host_tool_edges_order_target_package_and_product_plans() {
    let fixture = Fixture::new();

    let graph = fixture
        .load(&mut GeneratorCache::default(), &Executor::new(outputs))
        .expect("graph");

    let plan = graph
        .build_plan(&NativeTarget::current().expect("host").identity())
        .expect("native build plan");

    assert_eq!(
        plan.packages()
            .iter()
            .map(|identity| identity.as_str())
            .collect::<Vec<_>>(),
        ["test.tool", "test.app"]
    );

    assert_eq!(
        plan.products()
            .iter()
            .map(|identity| identity.package().as_str())
            .collect::<Vec<_>>(),
        ["test.tool", "test.app"]
    );
}

#[test]
fn tool_reference_failures_retain_the_known_package_product_or_target_cause() {
    for (field, value, expected) in [
        (
            "package",
            "test.absent",
            bray_project::ProjectManifestProblemKind::UnknownDependencyPackage,
        ),
        (
            "product",
            "absent",
            bray_project::ProjectManifestProblemKind::UnknownDependencyProduct,
        ),
        (
            "target",
            "portable",
            bray_project::ProjectManifestProblemKind::DependencyTargetUnavailable,
        ),
    ] {
        let mut fixture = Fixture::new();

        let other = if NativeTarget::current().expect("host").as_str() == "aarch64-pc-windows-msvc"
        {
            "x86_64-unknown-linux-gnu"
        } else {
            "aarch64-pc-windows-msvc"
        };

        fixture.workspace["targets"]
            .as_array_mut()
            .expect("array")
            .push(json!({"name": "portable", "identity": other}));

        fixture.package["generators"][0]["tool"][field] = json!(value);
        fixture.save();

        let result = fixture.load(&mut GeneratorCache::default(), &Executor::new(outputs));

        assert!(
            matches!(result, Err(ProjectLoadError::InvalidManifest { problem, .. }) if problem.kind() == expected)
        );
    }
}
