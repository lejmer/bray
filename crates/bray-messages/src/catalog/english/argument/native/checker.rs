pub(super) fn format_english_checker_failure(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> String {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    let message = match failure {
        Failure::MissingSource { source_id } => {
            return format!(
                "an internal compiler error prevented Bray from finding source snapshot #{} required by this product",
                source_id.raw(),
            );
        }
        Failure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => {
            return format!(
                "an internal compiler error expected revision {} of source snapshot #{} but found revision {} while compiling this product",
                expected.raw(),
                source_id.raw(),
                actual.raw(),
            );
        }
        Failure::InvalidSourceRange { span } => {
            return format!(
                "an internal compiler error retained bytes {} through {} outside source snapshot #{}",
                span.start().bytes(),
                span.end().bytes(),
                span.source_id().raw(),
            );
        }
        Failure::SemanticQueryUnavailable { symbol, query } => {
            return format!(
                "an internal compiler error could not obtain {} for {} declaration #{}",
                format_checker_query(query),
                format_checker_symbol_kind(symbol.kind()),
                symbol.ordinal(),
            );
        }
        Failure::SemanticValueUnavailable => {
            "an internal compiler error prevented Bray from retaining declaration information required by this product"
        }
        Failure::AtomicRepresentationTypeUnavailable => {
            "the selected atomic value has no available representation type"
        }
        Failure::AtomicRepresentationArgumentsUnavailable => {
            "the selected atomic value has no available representation arguments"
        }
        Failure::AtomicInitializerArgumentUnavailable => {
            "the atomic initializer argument has no compile-time value"
        }
        Failure::AtomicInitializerResultUnavailable => {
            "the atomic initializer result cannot be represented as a compile-time value"
        }
        Failure::UninitInitializerResultUnavailable => {
            "the uninitialized-storage initializer result cannot be represented as a compile-time value"
        }
        Failure::ImportedExecutableTemplateMismatch => {
            "an imported native operation does not match its compiled definition"
        }
        Failure::CompilerKnownRepresentationUnavailable(role) => {
            return format!(
                "an internal compiler error prevented Bray from finding the language-defined `{}` type required for the selected target",
                format_compiler_known_representation(role),
            );
        }
        Failure::InvalidExpressionTypeInput { expression } => {
            return format!(
                "an internal compiler error associated expression #{} in source body #{} with the wrong source body while determining its type",
                expression.ordinal(),
                expression.unit(),
            );
        }
        Failure::InvalidSemanticSelectionInput => {
            "an internal compiler error prevented Bray from selecting the operation or call for an expression"
        }
        Failure::InvalidLiteralValueInput => {
            "an internal compiler error associated a literal with the wrong source body while determining its value"
        }
        Failure::InvalidConstantEvaluationInput => {
            "an internal compiler error supplied incompatible source information while evaluating a constant expression"
        }
        Failure::InvalidPatternCheckInput => {
            "an internal compiler error supplied incompatible source information while checking a pattern"
        }
        Failure::InvalidStoragePlan => {
            "an internal compiler error prevented Bray from arranging the local values used by a source body"
        }
        Failure::InvalidLiveness => {
            "an internal compiler error prevented Bray from determining how long a value remains usable"
        }
        Failure::InvalidRefinementInput => {
            "an internal compiler error prevented Bray from tracking what a condition or pattern proves about a value"
        }
        Failure::RefinementCapacityUnrepresentable => {
            "an internal compiler limit prevented Bray from retaining everything a condition or pattern proves about a value"
        }
        Failure::RefinementStorageUnavailable => {
            "the compiler could not allocate memory needed to track what a condition or pattern proves about a value"
        }
        Failure::InvalidStorageFlow => {
            "an internal compiler error prevented Bray from tracking how a source body uses its values"
        }
        Failure::InvalidBodySemantics => {
            "an internal compiler error found incompatible analysis results for a source body"
        }
        Failure::InvalidBoundNode { node } => {
            return format!(
                "an internal compiler error lost {} #{} required to compile source body #{}",
                format_checker_node_kind(node.kind()),
                node.ordinal(),
                node.unit(),
            );
        }
        Failure::ExpressionTypeCapacityExceeded => {
            "an internal compiler limit prevented Bray from determining every expression type in a source body"
        }
        Failure::InvalidUnitView("semantic_context_mismatch") => {
            "an internal compiler error associated analysis results with the wrong source declaration or body"
        }
        Failure::InvalidUnitView(reason) => {
            return format!(
                "an internal compiler error could not use a source declaration or body because its `{reason}` consistency check failed"
            );
        }
    };

    message.to_owned()
}

fn format_checker_query(query: &str) -> &str {
    match query {
        "members" => "its member declarations",
        "imports" => "its imported declarations",
        "directives" => "its declaration directives",
        "generic_parameters" => "its generic parameters",
        "generic_declaration_template" => "its generic declaration",
        "generic_constraints" => "its checked generic constraints",
        "callable_signature" => "its callable signature",
        "callable_contracts" => "its callable contracts",
        "callable_contract_template" => "its callable contract declaration",
        "predicate_signature_template" => "its predicate signature",
        "callable_contract_type" => "its callable contract type",
        "constant_declared_type" => "its declared constant type",
        "constant_definition" => "its checked constant definition",
        "static_instance_template" => "its static value definition",
        "callable_parameter_default" => "its checked parameter default",
        "unevaluated_default_template" => "its default value expression",
        "struct_field_type" => "its field type",
        "type_member_value" => "its type member value",
        "struct_field_default" => "its checked field default",
        "union_payload_field_type" => "its payload field type",
        "union_payload_field_default" => "its checked payload field default",
        "predicate_definition" => "its checked predicate definition",
        "union_variant_payload" => "its variant payload",
        "implementation_subject" => "its implementation subject",
        "implemented_trait_application" => "its implemented trait",
        "implementation_head_template" => "its implementation declaration",
        "implementation_coherence" => "its implementation coherence information",
        "overload_arms" => "its resolved overload alternatives",
        "overload_signature_template" => "its overload signatures",
        query => query,
    }
}

fn format_checker_symbol_kind(kind: &str) -> &str {
    match kind {
        "callable_overload" | "implementation_overload" => "overload",
        "callable_parameter" => "parameter",
        "inherent_implementation" | "named_trait_implementation"
        | "unnamed_trait_implementation" => "implementation",
        "struct_field" => "struct field",
        "trait_callable_fulfillment" | "trait_callable_member" | "type_callable_member" => {
            "callable member"
        }
        "trait_constant_fulfillment" | "trait_constant_member" => "constant member",
        "trait_type_valued_fulfillment" | "trait_type_valued_member" => "type member",
        "union_payload_field" => "union payload field",
        "union_variant" => "union variant",
        kind => kind,
    }
}

fn format_checker_node_kind(kind: &str) -> &str {
    match kind {
        "callable_body" => "callable body",
        kind => kind,
    }
}

fn format_compiler_known_representation(role: &str) -> &str {
    match role {
        "ScalarBool" => "bool",
        "ScalarChar" => "char",
        "ScalarI8" => "i8",
        "ScalarI16" => "i16",
        "ScalarI32" => "i32",
        "ScalarI64" => "i64",
        "ScalarI128" => "i128",
        "ScalarU8" => "u8",
        "ScalarU16" => "u16",
        "ScalarU32" => "u32",
        "ScalarU64" => "u64",
        "ScalarU128" => "u128",
        "ScalarIsize" => "isize",
        "ScalarUsize" => "usize",
        "ScalarR16" => "r16",
        "ScalarR32" => "r32",
        "ScalarR64" => "r64",
        "ScalarR128" => "r128",
        "ScalarC32" => "c32",
        "ScalarC64" => "c64",
        "ScalarC128" => "c128",
        "ScalarC256" => "c256",
        role => role,
    }
}
