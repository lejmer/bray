use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiType};
use quote::ToTokens;

pub(super) fn validate(root: &Path) -> Result<(), String> {
    let mut paths = Vec::new();

    crate::workspace::collect_rust_files(
        &root.join("crates/bray-runtime-adapter/src"),
        &mut paths,
    )?;

    paths.sort();

    let known_symbols = RuntimeAbiRole::ALL
        .into_iter()
        .filter(|role| role.bootstrap_declaration().is_none())
        .filter_map(RuntimeAbiRole::native_symbol)
        .chain(
            super::command::RuntimeArchiveKind::ALL
                .into_iter()
                .flat_map(super::archive::support_exports),
        )
        .collect::<BTreeSet<_>>();

    let mut implementations = BTreeMap::new();

    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| crate::workspace::io_error("read", &path, error))?;

        let file = syn::parse_file(&source)
            .map_err(|error| format!("could not parse {}: {error}", path.display()))?;

        for item in file.items {
            let syn::Item::Macro(item) = item else {
                continue;
            };

            if !item.mac.path.is_ident("native_adapter") {
                continue;
            }

            let function = syn::parse2::<syn::ItemFn>(item.mac.tokens).map_err(|error| {
                format!("invalid native adapter in {}: {error}", path.display())
            })?;

            let name = function.sig.ident.to_string();

            if !known_symbols.contains(name.as_str()) {
                return Err(format!(
                    "{}: native adapter {name} has no runtime role or support-export owner",
                    path.display(),
                ));
            }

            implementations
                .entry(name)
                .or_insert_with(Vec::new)
                .push((path.clone(), function.sig));
        }
    }

    for role in RuntimeAbiRole::ALL {
        let Some(symbol) = role.native_symbol() else {
            continue;
        };

        if role.bootstrap_declaration().is_some() {
            continue;
        }

        let signatures = implementations.get(symbol).ok_or_else(|| {
            format!(
                "runtime role {} has no native adapter for {symbol}",
                role.as_str()
            )
        })?;

        for (path, signature) in signatures {
            validate_signature(role, signature)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }

    Ok(())
}

fn validate_signature(role: RuntimeAbiRole, signature: &syn::Signature) -> Result<(), String> {
    let expected = role.native_signature().ok_or_else(|| {
        format!(
            "compiler-owned role {} cannot have a native adapter",
            role.as_str()
        )
    })?;

    let abi = signature
        .abi
        .as_ref()
        .and_then(|abi| abi.name.as_ref())
        .map(syn::LitStr::value);

    if !matches!(abi.as_deref(), Some("C" | "C-unwind")) || signature.variadic.is_some() {
        return Err(format!(
            "runtime role {} requires a non-variadic C ABI",
            role.as_str()
        ));
    }

    if signature.asyncness.is_some() || !signature.generics.params.is_empty() {
        return Err(format!(
            "runtime role {} requires a synchronous non-generic adapter",
            role.as_str()
        ));
    }

    let parameters = signature
        .inputs
        .iter()
        .map(|argument| {
            let syn::FnArg::Typed(argument) = argument else {
                return Err(format!("runtime role {} has a receiver", role.as_str()));
            };

            native_type(&argument.ty)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("runtime role {}: {error}", role.as_str()))?;

    let result = match &signature.output {
        syn::ReturnType::Default => RuntimeAbiType::Void,
        syn::ReturnType::Type(_, ty) => {
            native_type(ty).map_err(|error| format!("runtime role {}: {error}", role.as_str()))?
        }
    };

    if parameters != expected.parameters() || result != expected.result() {
        return Err(format!(
            "runtime role {} has signature {parameters:?} -> {result:?}, expected {:?} -> {:?}",
            role.as_str(),
            expected.parameters(),
            expected.result(),
        ));
    }

    Ok(())
}

fn native_type(ty: &syn::Type) -> Result<RuntimeAbiType, String> {
    if native_callback(ty) {
        return Ok(RuntimeAbiType::Pointer);
    }

    if let Some(inner) = optional_type(ty) {
        if let syn::Type::Reference(reference) = inner {
            return pointer_kind(&reference.elem);
        }

        if native_callback(inner) {
            return Ok(RuntimeAbiType::Pointer);
        }
    }

    let name = match ty {
        syn::Type::Never(_) => return Ok(RuntimeAbiType::Never),
        syn::Type::Ptr(pointer) => return pointer_kind(&pointer.elem),
        syn::Type::Reference(reference) => return pointer_kind(&reference.elem),
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    };

    Ok(match name.as_deref() {
        Some("u8") => RuntimeAbiType::U8,
        Some("u32" | "NativeRuntimeStatus" | "NativeRunState" | "NativeProductHostOperation") => {
            RuntimeAbiType::U32
        }
        Some("u64" | "NativeRootHandle" | "NativeTaskHandle") => RuntimeAbiType::U64,
        Some("usize" | "NativeProtectedFrameTransfer") => RuntimeAbiType::Usize,
        Some("NativeRuntimeConfiguration") => RuntimeAbiType::Configuration,
        Some("NativeRootStart") => RuntimeAbiType::RootStart,
        Some("NativeRunOutcome") => RuntimeAbiType::RunOutcome,
        Some("NativeTaskAllocation") => RuntimeAbiType::TaskAllocation,
        Some("NativeInactiveFrame") => RuntimeAbiType::InactiveFrame,
        Some("NativeFrameProgress") => RuntimeAbiType::FrameProgress,
        Some("NativeExecutionLaneResult") => RuntimeAbiType::LaneResult,
        Some("NativeProductHostObservation") => RuntimeAbiType::ProductObservation,
        _ => {
            return Err(format!(
                "unsupported native adapter type {}",
                ty.to_token_stream()
            ));
        }
    })
}

fn pointer_kind(ty: &syn::Type) -> Result<RuntimeAbiType, String> {
    if matches!(ty, syn::Type::Slice(_) | syn::Type::TraitObject(_))
        || matches!(ty, syn::Type::Path(path) if path.path.is_ident("str"))
    {
        return Err(format!(
            "native adapter pointer has unsized target {}",
            ty.to_token_stream()
        ));
    }

    if matches!(ty, syn::Type::Path(path) if path.path.is_ident("usize")) {
        Ok(RuntimeAbiType::PointerUsize)
    } else {
        Ok(RuntimeAbiType::Pointer)
    }
}

fn native_callback(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };

    path.path.segments.last().is_some_and(|segment| {
        matches!(segment.arguments, syn::PathArguments::None)
            && matches!(
                segment.ident.to_string().as_str(),
                "NativeWakeCallback" | "NativeRuntimeEventCallback" | "NativeFrameMetadataProvider"
            )
    })
}

fn optional_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };

    let segment = path.path.segments.last()?;

    if segment.ident != "Option" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };

    match arguments.args.first() {
        Some(syn::GenericArgument::Type(inner)) if arguments.args.len() == 1 => Some(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeAbiRole;

    use super::{native_type, validate, validate_signature};

    #[test]
    fn undeclared_native_adapters_fail_even_without_a_reserved_prefix() {
        let directory = tempfile::tempdir().expect("test directory exists");
        let sources = directory.path().join("crates/bray-runtime-adapter/src");
        std::fs::create_dir_all(&sources).expect("adapter directory exists");
        let source = sources.join("unprojected.rs");

        std::fs::write(
            &source,
            "native_adapter! { pub extern \"C\" fn forgotten_adapter() {} }",
        )
        .expect("adapter source writes");

        assert_eq!(
            validate(directory.path()),
            Err(format!(
                "{}: native adapter forgotten_adapter has no runtime role or support-export owner",
                source.display(),
            )),
        );
    }

    #[test]
    fn wide_pointers_are_not_native_addresses() {
        for source in [
            "*const [u8]",
            "&str",
            "*mut dyn Send",
            "Option<&[u8]>",
            "Option<&str>",
            "Option<&mut dyn Send>",
        ] {
            let ty = syn::parse_str::<syn::Type>(source).expect("type parses");

            assert!(native_type(&ty).is_err(), "{source} is a wide pointer");
        }
    }

    #[test]
    fn nullable_references_preserve_the_pointee_abi_category() {
        use bray_runtime_interface::RuntimeAbiType;

        for (source, expected) in [
            (
                "Option<&bray_runtime_abi::NativeValueCleanup>",
                RuntimeAbiType::Pointer,
            ),
            ("std::option::Option<&mut u8>", RuntimeAbiType::Pointer),
            ("Option<&usize>", RuntimeAbiType::PointerUsize),
        ] {
            let ty = syn::parse_str::<syn::Type>(source).unwrap();

            assert_eq!(native_type(&ty), Ok(expected));
        }

        for source in ["Option<*const u8>", "Option<usize>", "Vec<&u8>"] {
            let ty = syn::parse_str::<syn::Type>(source).unwrap();

            assert!(
                native_type(&ty).is_err(),
                "{source} has no guaranteed nullable-reference ABI"
            );
        }
    }

    #[test]
    fn nullable_native_callbacks_use_the_function_pointer_abi() {
        for name in [
            "NativeWakeCallback",
            "NativeRuntimeEventCallback",
            "NativeFrameMetadataProvider",
        ] {
            for source in [
                format!("bray_runtime_abi::{name}"),
                format!("Option<bray_runtime_abi::{name}>"),
            ] {
                let ty = syn::parse_str::<syn::Type>(&source).unwrap();

                assert_eq!(
                    native_type(&ty),
                    Ok(bray_runtime_interface::RuntimeAbiType::Pointer)
                );
            }
        }

        for source in [
            "Option<Option<NativeFrameMetadataProvider>>",
            "Option<NativeFrameMetadata>",
            "NativeFrameMetadataProvider<u32>",
        ] {
            let ty = syn::parse_str::<syn::Type>(source).unwrap();

            assert!(
                native_type(&ty).is_err(),
                "{source} is not a native callback pointer"
            );
        }
    }

    #[test]
    fn every_reference_runtime_adapter_matches_the_catalog() {
        let root = crate::workspace::root().expect("workspace exists");

        validate(&root).expect("runtime adapters must match the role catalog");
    }

    #[test]
    fn role_signature_mismatches_identify_the_role_and_shapes() {
        let signature = syn::parse_str::<syn::ItemFn>(
            "pub extern \"C\" fn bray_runtime_task_event_creation() -> u32 { 0 }",
        )
        .expect("signature parses")
        .sig;

        let error = validate_signature(RuntimeAbiRole::TaskEventCreation, &signature)
            .expect_err("wrong result kind must fail");

        assert_eq!(
            error,
            "runtime role task_event_creation has signature [] -> U32, expected [] -> Usize"
        );
    }

    #[test]
    fn parameter_and_calling_convention_mismatches_are_rejected() {
        for source in [
            "pub extern \"C\" fn operation(value: u32) -> u32 { value }",
            "pub fn operation(value: usize) -> u32 { 0 }",
            "pub async extern \"C\" fn operation(value: usize) -> u32 { 0 }",
            "pub extern \"C\" fn operation<T>(value: usize) -> u32 { 0 }",
        ] {
            let signature = syn::parse_str::<syn::ItemFn>(source)
                .expect("signature parses")
                .sig;

            assert!(validate_signature(RuntimeAbiRole::TaskEventSignal, &signature).is_err());
        }
    }
}
