use std::collections::BTreeSet;
use std::sync::Arc;

use bray_compiler_known::RecognizedStandardLibraryDeclarationKey;
use bray_ir::{MirOperationKind, MirProjectionKind};
use bray_package_interface::{
    InterfaceCheckedTemplateOperation, InterfaceLanguageRevision, InterfaceProductIdentity,
    InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
    ValidatedPackageInterface, build_package_interface_surface, encode_package_interface,
};
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    InherentImplementationSymbolId, MemberLookupResult, ModulePathKey, PackageIdentity,
    ProductKind, TypeCallableMemberSymbolId,
};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};
use bray_testing::test_source_inputs;

use crate::test_support::{
    package_version, source_function_body_key, source_named_trait_callable_fulfillment_body_key,
};
use crate::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
};

use super::fixtures::{compilation_from_sources_for_product_with_platform_services, export};

#[test]
fn platform_service_implementations_publish_their_role_with_the_root_template() {
    let Some(binding) =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
    else {
        panic!("test platform binding must be valid");
    };

    let compilation = compilation_from_sources_for_product_with_platform_services(
        [r#"
            trusted module app;

            @layout(c)
            internal struct PlatformStatus
            {
                category: u32;
                reserved: u32;
                native_code: i64;
            }

            @abi(c)
            trusted internal func flush() -> PlatformStatus
            {
                return
                {
                    category = 0,
                    reserved = 0,
                    native_code = 0
                };
            }
        "#],
        ProductKind::Library,
        [binding],
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let platform_templates = bundle
        .executable_templates()
        .iter()
        .filter(|template| template.platform_service().is_some())
        .collect::<Vec<_>>();

    assert_eq!(platform_templates.len(), 1);

    assert_eq!(
        platform_templates[0].identity(),
        bray_ir::MirExecutableTemplateId::ROOT
    );

    assert_eq!(
        platform_templates[0].platform_service(),
        Some(PlatformServiceRole::StandardOutputFlush)
    );
}

#[test]
fn standard_memory_surface_exports_uninitialized_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory.bray"),
    ]);

    let source_graph = compilation
        .product_source_graph()
        .unwrap_or_else(|error| panic!("standard memory source graph must build: {error:?}"));

    assert!(
        source_graph.diagnostics().is_empty(),
        "standard memory source graph diagnostics: {:?}",
        source_graph.diagnostics()
    );

    let product = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("standard memory product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "standard memory product diagnostics: {:?}",
        product.diagnostics()
    );

    assert!(
        !product.value().is_recovered(),
        "standard memory product semantics must not recover"
    );

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("standard memory symbol graph must build: {error:?}"));

    let identity = super::super::construction::build_identity_surface(
        &compilation,
        symbols,
        product.value().public_symbols(),
    )
    .unwrap_or_else(|error| panic!("standard memory identity surface must build: {error:?}"));

    let request = compilation
        .package_interface_export_request()
        .unwrap_or_else(|| panic!("standard memory export request must exist"));

    let surface = build_package_interface_surface(
        request.identity().clone(),
        [],
        identity.symbols,
        identity.relationships,
        identity.exports,
    )
    .unwrap_or_else(|error| panic!("standard memory interface surface must build: {error:?}"));

    let (semantics, _, _, _) = super::super::super::semantic::build_semantics(
        &compilation,
        symbols,
        &surface,
        &identity.selected,
        &identity.keys,
    )
    .unwrap_or_else(|error| panic!("standard memory semantic export must build: {error:?}"));

    for (template_index, template) in semantics.checked_templates().iter().enumerate() {
        for (node_index, node) in template.nodes().iter().enumerate() {
            let InterfaceCheckedTemplateOperation::Borrow { kind, operand } = node.operation()
            else {
                continue;
            };

            let operand_index = usize::try_from(operand.raw()).unwrap_or_else(|error| {
                panic!(
                    "standard memory borrow operand for template {template_index} node {node_index} must fit: {error:?}"
                )
            });

            let operand_ty = template.nodes()[operand_index].ty();

            let node_ty = semantics
                .types()
                .iter()
                .enumerate()
                .find_map(|(index, ty)| {
                    u32::try_from(index)
                        .ok()
                        .filter(|index| {
                            bray_package_interface::InterfaceTypeId::new(*index) == node.ty()
                        })
                        .map(|_| ty)
                });

            assert!(
                matches!(
                    node_ty,
                    Some(bray_package_interface::InterfaceType::Borrow {
                        kind: type_kind,
                        target,
                    }) if *type_kind == *kind && *target == operand_ty
                ),
                "standard memory borrow template {template_index} node {node_index} kind {kind:?} operand type {operand_ty:?} must match node type {node_ty:?}"
            );
        }
    }

    assert_strictly_canonical("constraints", semantics.constraints());
    assert_strictly_canonical("callable contracts", semantics.callable_contracts());
    assert_strictly_canonical("callable signatures", semantics.callable_signatures());
    assert_strictly_canonical("generic declarations", semantics.generic_declarations());

    assert_strictly_canonical(
        "callable parameter defaults",
        semantics.callable_parameter_defaults(),
    );

    assert_strictly_canonical("predicate definitions", semantics.predicate_definitions());
    assert_strictly_canonical("declared types", semantics.declared_types());
    assert_strictly_canonical("type representations", semantics.type_representations());
    assert_strictly_canonical("implementations", semantics.implementations());
    assert_strictly_canonical("coherence", semantics.coherence());
    assert_strictly_canonical("target dependencies", semantics.target_dependencies());
    assert_strictly_canonical("ABI dependencies", semantics.abi_dependencies());
    assert_strictly_canonical("runtime requirements", semantics.runtime_requirements());
    assert_strictly_canonical("provenance", semantics.provenance());

    compilation
        .package_implementation_configuration(None)
        .unwrap_or_else(|error| {
            panic!("standard memory implementation configuration must build: {error:?}")
        });

    let _ = export(&compilation);
}

#[test]
fn standard_memory_api_fixture_checks() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../../../standard-library/std/tests/api/memory.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "standard memory API diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let mut recovered = Vec::new();

    walk_syntax_tree(compilation.syntax_tree(), |event| {
        if let SyntaxWalkEvent::EnterNode(node) = event
            && node.is_recovered()
        {
            recovered.push((node.kind(), node.full_range()));
        }

        SyntaxWalkControl::Continue
    });

    assert!(
        recovered.is_empty(),
        "standard memory API syntax must not recover: {recovered:?}"
    );
}

#[test]
fn standard_string_equality_satisfies_source_and_imported_generic_constraints() {
    let provider = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/string.bray"),
        r#"
            module bray.standard_library_tests.string_operations;

            using std.string.StringEquatable;

            func generic_equal<T>(pos left: T, pos right: T) -> bool
                with(T: Equatable<T>)
            {
                return left == right;
            }

            func source_string_equality()
            {
                assert(generic_equal<string>("same", "same"));
            }
        "#,
    ]);

    assert!(
        provider.check_diagnostics().is_empty(),
        "source standard-library equality diagnostics: {:#?}",
        provider.check_diagnostics()
    );

    let artifact = encode_package_interface(export(&provider))
        .unwrap_or_else(|error| panic!("string interface must encode: {error:?}"));

    let standard_library = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        standard_library,
        product,
        "std.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer_package = PackageIdentity::try_new("std.tests.api")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        r#"
            module bray.standard_library_tests.string_operations;

            using std.string.StringEquatable;

            func generic_equal<T>(pos left: T, pos right: T) -> bool
                with(T: Equatable<T>)
            {
                return left == right;
            }

            func imported_string_equality()
            {
                assert(generic_equal<string>("same", "same"));
            }
        "#,
    );

    let consumer = Compilation::load(
        CompilationRequest::new(consumer_package, vec![source])
            .with_dependency_interfaces([dependency])
            .with_standard_library_source_authority(),
    )
    .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.check_diagnostics().is_empty(),
        "imported standard-library equality diagnostics: {:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn standard_formatting_surface_round_trips_and_specializes_without_provider_source() {
    // This interface fixture never executes its buffer adapters. Give them explicit diverging
    // bodies so imported reachability cannot silently treat Bray declarations as foreign imports.
    let provider = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        r#"
            module std.memory;

            union MemoryLayoutError
            {
                SizeOverflow;
                UnsupportedAlignment;
            }

            func byte_slice_pointer(pos bytes: &[u8]) -> RawPointer<u8>
            {
                loop
                {
                }
            }

            func byte_slice_pointer_mut(pos bytes: &mut [u8]) -> RawPointer<u8>
            {
                loop
                {
                }
            }

            trusted func byte_buffer_copy(pos source: RawPointer<u8>, pos destination: RawPointer<u8>, count: usize)
            {
                loop
                {
                }
            }

            trusted func byte_buffer_fill(destination: RawPointer<u8>, value: u8, count: usize)
            {
                loop
                {
                }
            }
        "#,
        r#"
            module std.bytes;

            using std.memory;

            struct Buffer
            {
                internal value: bool;

                internal construct(capacity: usize = 0) -> Result<Self, std.memory.MemoryLayoutError>
                {
                    let buffer: Buffer =
                    {
                        value = false,
                    };

                    return Ok(buffer);
                }

                func as_slice() -> &[u8]
                {
                    return as_slice(&self);
                }
            }

            func as_slice(pos buffer: &Buffer) -> &[u8]
            {
                loop
                {
                }
            }

            func length(pos buffer: &Buffer) -> usize
            {
                loop
                {
                }
            }

            func push(pos buffer: &mut Buffer, value: u8) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            internal func append_slice(pos buffer: &mut Buffer, pos bytes: &[u8]) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            internal func append_repeated(
                pos buffer: &mut Buffer,
                pos value: u8,
                pos count: usize
            ) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            overload append =
            {
                append_slice,
                append_repeated,
            }

            func reserve(pos buffer: &mut Buffer, additional: usize) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            func resize(pos buffer: &mut Buffer, new_length: usize, fill: u8 = 0) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }
        "#,
        r#"
            module std.string;

            union Utf8Error
            {
                InvalidEncoding;
            }

            impl string
            {
                func as_bytes() -> &[u8]
                {
                    return internal utf8(&self);
                }

                static func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>
                {
                    return internal decode_utf8(bytes);
                }
            }

            internal func utf8(pos value: &string) -> &[u8]
            {
                loop
                {
                }
            }

            internal func decode_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>
            {
                loop
                {
                }
            }
        "#,
        include_str!("../../../../../../../standard-library/std/src/character.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/checked.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/options.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/argument.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/sink.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/integer_width.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/rendering.bray"),
        r#"
            trusted module std.io;

            union IoErrorKind
            {
                BrokenStream;
            }

            struct IoError
            {
                kind: IoErrorKind;
                transferred: usize;
            }

            trait Writer
            {
                mut func write(pos source: &[u8]) -> Result<usize, IoError>
                    requires(blocking_execution());

                mut func flush() -> Result<unit, IoError>
                    requires(blocking_execution());

                mut func write_all(pos source: &[u8]) -> Result<unit, IoError>
                    requires(blocking_execution())
                {
                    let length: usize = source.length();
                    let mut written: usize = 0;

                    while written < length
                    {
                        let result: Result<usize, IoError> = self.write(&source[written..length]);

                        match consume result
                        {
                            case Ok(count)
                            {
                                if count == 0 || count > length - written
                                {
                                    return Error(
                                        {
                                            kind = IoErrorKind.BrokenStream,
                                            transferred = written
                                        }
                                    );
                                }

                                written += count;
                            }
                            case Error(error)
                            {
                                return Error(prefixed_error(error, prefix = written));
                            }
                        }
                    }

                    return Ok(unit);
                }
            }

            internal func smaller(pos left: usize, pos right: usize) -> usize
            {
                if left < right
                {
                    return left;
                }

                return right;
            }

            internal func prefixed_error(pos error: IoError, prefix: usize) -> IoError
            {
                return
                {
                    kind = error.kind,
                    transferred = prefix + error.transferred
                };
            }
        "#,
        include_str!("../../../../../../../standard-library/std/src/io/formatting.bray"),
        crate::test_support::RUNTIME_MEMORY_SOURCE,
        crate::test_support::RUNTIME_TEXT_SOURCE,
        crate::test_support::RUNTIME_CHARACTER_SOURCE,
    ]);

    assert!(
        provider.syntax_tree_result().diagnostics().is_empty(),
        "{:?}",
        provider.syntax_tree_result().diagnostics()
    );

    assert!(
        provider.declaration_diagnostics().is_empty(),
        "{:?}",
        provider.declaration_diagnostics()
    );

    let product = provider
        .product_semantics()
        .unwrap_or_else(|error| panic!("formatting product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "{:?}",
        product.diagnostics()
    );

    assert!(!product.value().is_recovered());

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let adapter = provider
        .lowered_unit(source_named_trait_callable_fulfillment_body_key(
            &provider,
            "WriterFormattingSink",
            "write",
        ))
        .unwrap_or_else(|error| panic!("writer formatting adapter must lower: {error:?}"));

    let adapter = adapter
        .value()
        .as_ref()
        .and_then(bray_lowering::LoweredUnit::mir)
        .unwrap_or_else(|| panic!("writer formatting adapter must produce MIR: {adapter:#?}"));

    assert!(
        adapter.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Borrow { place, .. }
                if place
                    .projections()
                    .iter()
                    .any(|projection| matches!(projection.kind(), MirProjectionKind::Field(_)))
                    && matches!(
                        place.projections().last().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
        )),
        "generic writer field borrow must reach the destination value: {adapter:#?}"
    );

    let interface = export(&provider);

    let runtime_capabilities: BTreeSet<_> = interface
        .semantics()
        .runtime_requirements()
        .iter()
        .flat_map(|requirement| requirement.requirements().capabilities())
        .copied()
        .collect();

    assert!(runtime_capabilities.is_empty());

    let artifact = encode_package_interface(interface)
        .unwrap_or_else(|error| panic!("formatting interface must encode: {error:?}"));

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated = ValidatedPackageInterface::try_new(artifact.bytes(), policy)
        .unwrap_or_else(|error| panic!("formatting interface must validate: {error:?}"));

    let implementation = PackageImplementationArtifact::try_new(
        &validated,
        interface.surface(),
        interface.semantics(),
        interface.implementation_configuration().clone(),
        [],
        interface.executable_templates().iter().cloned(),
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("formatting implementation must encode: {error:?}"));

    let provider_package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let provider_product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        provider_package.clone(),
        provider_product,
        "std.brayi",
        artifact.shared_bytes(),
        policy,
    )
    .with_implementation_artifact("std.brayimpl", Arc::new(implementation));

    let consumer_package = PackageIdentity::try_new("example.application")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        r#"
            module app;

            using std.format;
            using std.format.ByteSinkFormatting;
            using std.format.StringFormat;
            using std.format.I32Format;
            using std.format.U32Format;
            using std.bytes;
            using std.io;
            using std.io.WriterFormattingSink;
            using std.memory;

            struct RecordingWriter
            {
                mut written: usize;
            }

            impl RecordingWriterIo = RecordingWriter(std.io.Writer)
            {
                mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
                    requires(blocking_execution())
                {
                    let length: usize = source.length();

                    self.written += length;
                    return Ok(length);
                }

                mut func flush() -> Result<unit, std.io.IoError>
                    requires(blocking_execution())
                {
                    return Ok(unit);
                }
            }

            func render(pos destination: &mut std.format.ByteSink, pos value: string) -> Result<unit, std.memory.MemoryLayoutError>
                requires(blocking_execution())
            {
                return std.format.write(destination, std.format.Argument<string>(&value));
            }

            func render_integer(
                pos destination: &mut std.format.ByteSink,
                pos value: i32
            ) -> Result<unit, std.memory.MemoryLayoutError>
                requires(blocking_execution())
            {
                return std.format.write(destination, std.format.Argument<i32>(&value));
            }

            func resolved_defaults() -> std.format.Options
            {
                return std.format.Options();
            }

            public trusted func stream_integer(pos writer: &mut RecordingWriter, pos value: u32) -> Result<unit, std.io.IoError>
                requires(blocking_execution())
            {
                let mut destination: std.io.FormattingSink<RecordingWriter> = std.io.FormattingSink<RecordingWriter>(writer);

                return trusted std.format.write_to<u32, std.io.FormattingSink<RecordingWriter>, std.io.IoError>(
                    &mut destination,
                    std.format.Argument<u32>(&value),
                );
            }
        "#,
    );

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Library,
        SelectedTarget::default(),
    );

    let request = CompilationRequest::with_options(consumer_package, vec![source], options)
        .with_dependency_interfaces([dependency]);

    let consumer = Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.imported_diagnostics().is_empty(),
        "{:?}",
        consumer.imported_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("formatting skeleton must build: {error:?}"));

    let skeleton = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton"));

    let recognized = Arc::clone(
        imported
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton")),
    )
    .recognize_standard_library(&provider_package, |_| true);

    let recognized_key = |value| {
        RecognizedStandardLibraryDeclarationKey::try_new(value)
            .unwrap_or_else(|| panic!("recognized standard-library key must be valid: {value}"))
    };

    assert!(
        recognized
            .declaration_symbol::<InherentImplementationSymbolId>(&recognized_key(
                "StandardStringImplementation",
            ))
            .is_some(),
        "the string implementation must retain its imported identity"
    );

    for key in ["StandardStringAsBytes", "StandardStringFromUtf8"] {
        assert!(
            recognized
                .declaration_symbol::<TypeCallableMemberSymbolId>(&recognized_key(key))
                .is_some(),
            "{key} must retain its nested imported identity"
        );
    }

    let package = skeleton
        .package_by_identity(&provider_package)
        .unwrap_or_else(|| panic!("standard-library package must be imported"));

    assert!(
        consumer
            .symbol_graph()
            .unwrap_or_else(|error| panic!("consumer source symbols must build: {error:?}"))
            .packages()
            .iter()
            .all(|package| package.identity() != &provider_package),
        "provider symbols must come only from the package interface"
    );

    let format_path = ModulePathKey::try_new(["format"])
        .unwrap_or_else(|| panic!("format module path must be valid"));

    let format = skeleton
        .module_by_path(package.id(), &format_path)
        .unwrap_or_else(|| panic!("format module must be imported"));

    for name in [
        "Argument",
        "ByteSink",
        "ByteSinkFormatting",
        "Options",
        "write",
        "write_to",
    ] {
        assert!(
            matches!(
                skeleton.lookup(format.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    let bytes_path = ModulePathKey::try_new(["bytes"])
        .unwrap_or_else(|| panic!("bytes module path must be valid"));

    let bytes = skeleton
        .module_by_path(package.id(), &bytes_path)
        .unwrap_or_else(|| panic!("bytes module must be imported"));

    assert!(matches!(
        skeleton.lookup(bytes.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let memory_path = ModulePathKey::try_new(["memory"])
        .unwrap_or_else(|| panic!("memory module path must be valid"));

    let memory = skeleton
        .module_by_path(package.id(), &memory_path)
        .unwrap_or_else(|| panic!("memory module must be imported"));

    assert!(matches!(
        skeleton.lookup(memory.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let io_path =
        ModulePathKey::try_new(["io"]).unwrap_or_else(|| panic!("io module path must be valid"));

    let io = skeleton
        .module_by_path(package.id(), &io_path)
        .unwrap_or_else(|| panic!("io module must be imported"));

    for name in ["IoError", "Writer", "WriterFormattingSink"] {
        assert!(
            matches!(
                skeleton.lookup(io.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let lowered = consumer
        .lowered_unit(source_function_body_key(&consumer, "resolved_defaults"))
        .unwrap_or_else(|error| panic!("imported named constructor must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(!skeleton.traits().is_empty());
    assert!(!skeleton.structures().is_empty());
    assert!(!skeleton.named_trait_implementations().is_empty());

    let streamed = consumer
        .lowered_unit(source_function_body_key(&consumer, "stream_integer"))
        .unwrap_or_else(|error| panic!("imported formatting adapter must lower: {error:?}"));

    assert!(streamed.value().is_some(), "{:#?}", streamed.diagnostics());

    assert!(
        streamed.diagnostics().is_empty(),
        "{:#?}",
        streamed.diagnostics()
    );

    let imported_instances = consumer
        .imported_codegen_instance_count_for_test()
        .unwrap_or_else(|error| panic!("imported formatting reachability must close: {error:?}"));

    assert!(imported_instances > 0);
}

fn assert_strictly_canonical<T>(table: &str, values: &[T])
where
    T: std::fmt::Debug + Ord,
{
    if let Some(pair) = values.windows(2).find(|pair| pair[0] >= pair[1]) {
        panic!(
            "standard memory {table} are not canonical: {:?} then {:?}",
            pair[0], pair[1]
        );
    }
}

fn standard_library_compilation<const N: usize>(sources: [&str; N]) -> Compilation {
    let package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        product,
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .unwrap_or_else(|| panic!("standard-library export identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let sources = test_source_inputs("standard", sources);

    let request = CompilationRequest::new(package, sources)
        .with_standard_library_source_authority()
        .with_package_interface_export(export);

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard-library compilation must load: {error:?}"))
}
