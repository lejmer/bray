pub(crate) const USER_FACING_SOURCES: &[(&str, &str)] = &[
    ("argument.rs", include_str!("../argument.rs")),
    ("argument/callable.rs", include_str!("callable.rs")),
    (
        "argument/callable/contract.rs",
        include_str!("callable/contract.rs"),
    ),
    (
        "argument/callable/overload.rs",
        include_str!("callable/overload.rs"),
    ),
    ("argument/emission.rs", include_str!("emission.rs")),
    (
        "argument/emission/failure.rs",
        include_str!("emission/failure.rs"),
    ),
    (
        "argument/emission/kind.rs",
        include_str!("emission/kind.rs"),
    ),
    ("argument/inspection.rs", include_str!("inspection.rs")),
    ("argument/interface.rs", include_str!("interface.rs")),
    (
        "argument/interface/inventory.rs",
        include_str!("interface/inventory.rs"),
    ),
    (
        "argument/interface/problem.rs",
        include_str!("interface/problem.rs"),
    ),
    (
        "argument/interface/source.rs",
        include_str!("interface/source.rs"),
    ),
    (
        "argument/interface/validation.rs",
        include_str!("interface/validation.rs"),
    ),
    ("argument/native.rs", include_str!("native.rs")),
    (
        "argument/native/artifact.rs",
        include_str!("native/artifact.rs"),
    ),
    (
        "argument/native/checker.rs",
        include_str!("native/checker.rs"),
    ),
    (
        "argument/native/dependency.rs",
        include_str!("native/dependency.rs"),
    ),
    (
        "argument/native/document.rs",
        include_str!("native/document.rs"),
    ),
    (
        "argument/native/external.rs",
        include_str!("native/external.rs"),
    ),
    (
        "argument/native/foreign_query.rs",
        include_str!("native/foreign_query.rs"),
    ),
    (
        "argument/native/linking.rs",
        include_str!("native/linking.rs"),
    ),
    (
        "argument/native/product_query.rs",
        include_str!("native/product_query.rs"),
    ),
    (
        "argument/native/standard_library.rs",
        include_str!("native/standard_library.rs"),
    ),
    (
        "argument/native/toolchain.rs",
        include_str!("native/toolchain.rs"),
    ),
    ("argument/project.rs", include_str!("project.rs")),
    (
        "argument/project/command.rs",
        include_str!("project/command.rs"),
    ),
    (
        "argument/project/dependency.rs",
        include_str!("project/dependency.rs"),
    ),
    (
        "argument/project/manifest.rs",
        include_str!("project/manifest.rs"),
    ),
    (
        "argument/project/selection.rs",
        include_str!("project/selection.rs"),
    ),
    ("argument/selection.rs", include_str!("selection.rs")),
    (
        "argument/selection/candidate.rs",
        include_str!("selection/candidate.rs"),
    ),
    (
        "argument/selection/kind.rs",
        include_str!("selection/kind.rs"),
    ),
    (
        "argument/selection/native.rs",
        include_str!("selection/native.rs"),
    ),
    ("argument/semantics.rs", include_str!("semantics.rs")),
    ("argument/source.rs", include_str!("source.rs")),
    ("argument/target.rs", include_str!("target.rs")),
    ("argument/value.rs", include_str!("value.rs")),
];
