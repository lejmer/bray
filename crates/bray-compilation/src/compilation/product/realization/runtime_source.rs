use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::CallableAbi;

use super::super::super::Compilation;
use super::super::super::foreign::{AbiField, abi_type_matches};
use crate::fact::{CancellationToken, FactQueryError};

const BYTE_POINTER: AbiField = AbiField::Pointer(RepresentationRole::ScalarU8);
const U32: AbiField = AbiField::Scalar(RepresentationRole::ScalarU32);
const U64: AbiField = AbiField::Scalar(RepresentationRole::ScalarU64);
const USIZE: AbiField = AbiField::Scalar(RepresentationRole::ScalarUsize);

macro_rules! define_runtime_source_fields {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        fn runtime_source_fields(role: RuntimeAbiRole) -> Option<(&'static [AbiField], AbiField)> {
            match role {
                $(RuntimeAbiRole::$role => define_runtime_source_fields!(
                    @bootstrap ($($bootstrap)?) ($([$($native_parameter),*] -> $native_result)?)
                ),)+
            }
        }
    };
    (@bootstrap () $native:tt) => { None };
    (@bootstrap ($name:literal) ([$($parameter:ident),*] -> $result:ident)) => {
        Some((&[$(define_runtime_source_fields!(@field $parameter),)*], define_runtime_source_fields!(@field $result)))
    };
    (@field U32) => { U32 };
    (@field U64) => { U64 };
    (@field Usize) => { USIZE };
    (@field Pointer) => { BYTE_POINTER };
}

bray_runtime_abi::runtime_role_catalog!(define_runtime_source_fields);

pub(super) fn runtime_source_signature_matches(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    signature: &CodegenCallableSignature,
    cancellation: &CancellationToken,
) -> Result<Option<bool>, FactQueryError> {
    let Some((parameters, result)) = runtime_source_fields(role) else {
        return Ok(None);
    };

    if signature.abi() != CallableAbi::C
        || signature.is_variadic()
        || signature.has_panic_report_context()
        || signature.parameters().len() != parameters.len()
    {
        return Ok(Some(false));
    }

    for (parameter, expected) in signature.parameters().iter().zip(parameters) {
        let CodegenParameterMapping::Direct {
            ty,
            extension: None,
            attributes,
        } = parameter
        else {
            return Ok(Some(false));
        };

        if !attributes.is_empty() || !abi_type_matches(compilation, *ty, expected, cancellation)? {
            return Ok(Some(false));
        }
    }

    let expected_result = &result;

    let CodegenResultMapping::Direct {
        ty,
        extension: None,
        attributes,
    } = signature.result()
    else {
        return Ok(Some(false));
    };

    Ok(Some(
        attributes.is_empty() && abi_type_matches(compilation, *ty, expected_result, cancellation)?,
    ))
}

#[cfg(test)]
mod tests {
    use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
    use bray_compiler_known::RepresentationRole;
    use bray_runtime_interface::RuntimeAbiRole;
    use bray_symbols::CallableAbi;

    use super::runtime_source_signature_matches;
    use crate::CancellationToken;
    use crate::test_support::compilation;

    #[test]
    fn bootstrap_initialization_requires_native_abi_and_pointer_width_parameters() {
        let compilation = compilation("module app;\n");

        let usize = compilation
            .codegen_representation_type(RepresentationRole::ScalarUsize)
            .unwrap();

        let u32 = compilation
            .codegen_representation_type(RepresentationRole::ScalarU32)
            .unwrap();

        for (abi, parameter, expected) in [
            (CallableAbi::C, usize, true),
            (CallableAbi::C, u32, false),
            (CallableAbi::Bray, usize, false),
        ] {
            let signature = CodegenCallableSignature::new(
                [
                    CodegenParameterMapping::direct(parameter, None, []),
                    CodegenParameterMapping::direct(parameter, None, []),
                ],
                CodegenResultMapping::direct(u32, None, []),
                abi,
                false,
            );

            assert_eq!(
                runtime_source_signature_matches(
                    &compilation,
                    RuntimeAbiRole::RuntimeInitialization,
                    &signature,
                    &CancellationToken::new(),
                )
                .unwrap(),
                Some(expected),
            );
        }
    }
}
