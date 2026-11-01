use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

const API: &str = r"
    trusted module api;
    using core.memory;

    struct Marker {}
    struct Receipt { marker: Marker; }
    struct State { mut count: u32; }
    func reference(pos value: &u32) {}
    struct Box<T> { value: T; }
    union Choice<T> { Value(pos value: T); Empty; }

    trusted predicate held(owner: &(Receipt?));
    trusted func make() -> Receipt? ensures(trusted held(owner = &result)) {
        let value: Receipt? = Receipt { marker = Marker {} };
        return value;
    }
    func observe(pos value: &(Receipt?)) requires(trusted held(owner = value)) {}

    @copy struct Stamp { epoch: u64; }
    trusted predicate stamped(owner: &Stamp);
    trusted func stamp() -> Stamp ensures(trusted stamped(owner = &result)) { return { epoch = 1 }; }
    func observe_stamp(pos value: &Stamp) requires(trusted stamped(owner = value)) {}

    func extract<T>(pos value: Box<T>) -> (u32, T) { return (0, value.value); }
    func pair<T>(pos left: T, pos right: T) -> (T, T) { return (left, right); }
    async func deferred_tuple<T>(pos value: T) -> (u32, T) { return (0, value); }
    func tuple<T>(pos value: T) -> (u32, T) { return (0, value); }
    func product<T>(pos value: T) -> Box<T> { return { value = value }; }
    func variant<T>(pos value: T) -> Choice<T> { return .Value(value); }
    func nullable<T>(pos value: T) -> T? { return value; }
    func array<T>(pos value: T) -> [T; 1] { return [value]; }
    func nested<T>(pos value: T) -> Box<(u32, T?)> {
        let optional: T? = value;
        let wrapped: (u32, T?) = (0, optional);
        return { value = wrapped };
    }
    func with_reference<T>(pos value: T, pos other: &u32) -> (T, &u32) {
        return (value, other);
    }
    func mutate<T>(pos value: T, pos state: &mut State) -> (u32, T) {
        state.count += 1;
        return (state.count, value);
    }
    func branches<T>(pos value: T, pos choice: bool) -> (u32, T) {
        if choice { return (1, value); }
        return (2, value);
    }

    struct Dependency { mut marker: Marker; }
    struct RelatedOwner { marker: Marker; }
    trusted predicate related(owner: &Marker, state: &Marker);
    trusted func make_related(pos state: &Dependency) -> RelatedOwner
        ensures(trusted marked(owner = &result.marker), trusted related(owner = &result.marker, state = &state.marker)) {
        return { marker = Marker {} };
    }
    func replace(pos value: RelatedOwner, pos state: &mut Dependency) -> (u32, RelatedOwner) {
        state.marker = Marker {};
        return (0, value);
    }
    func keep(pos value: RelatedOwner, pos state: &mut Dependency) -> (u32, RelatedOwner) { return (0, value); }
    func observe_independent(pos value: &RelatedOwner) requires(trusted marked(owner = &value.marker)) {}
    func observe_related(pos value: &RelatedOwner, pos state: &Dependency)
        requires(trusted related(owner = &value.marker, state = &state.marker)) {}

    struct Address { marker: Marker; pointer: DataPointer<u32>; }
    trusted predicate marked(owner: &Marker);
    trusted func address(pos value: &u32) -> Address
        ensures(
            trusted marked(owner = &result.marker),
            trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
        ) {
        return { marker = Marker {}, pointer = core.memory.address_of<u32>(value) };
    }
    func observe_marker(pos value: &Address) requires(trusted marked(owner = &value.marker)) {}
    func observe_address(pos value: &Address)
        requires(trusted core.memory.valid_read<u32>(pointer = value.pointer, count = 1)) {}
";

fn check_owned_transfers(imported: bool) {
    let provider = compilation(API);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let cases = [
        (
            "deferred",
            "let result = await api.deferred_tuple<(api.Receipt?)>(INPUT); api.observe(&result.1);",
        ),
        (
            "tuple",
            "let result = api.tuple<(api.Receipt?)>(INPUT); api.observe(&result.1);",
        ),
        (
            "product",
            "let result = api.product<(api.Receipt?)>(INPUT); api.observe(&result.value);",
        ),
        (
            "union",
            "let result = api.variant<(api.Receipt?)>(INPUT); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "nullable",
            "let result = api.nullable<(api.Receipt?)>(INPUT); match consume result { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "array",
            "let result = api.array<(api.Receipt?)>(INPUT); let [value] = result; api.observe(&value);",
        ),
        (
            "nested",
            "let result = api.nested<(api.Receipt?)>(INPUT); match consume result.value.1 { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "reference",
            "let other: u32 = 3; let result = api.with_reference<(api.Receipt?)>(INPUT, &other); api.observe(&result.0); api.reference(result.1);",
        ),
        (
            "mutation",
            "let result = { let mut state: api.State = { count = 0 }; yield api.mutate<(api.Receipt?)>(INPUT, &mut state); }; api.observe(&result.1);",
        ),
        (
            "branches",
            "let result = api.branches<(api.Receipt?)>(INPUT, choice); api.observe(&result.1);",
        ),
        (
            "composition",
            "let result = api.product<(u32, api.Receipt?)>(api.tuple<(api.Receipt?)>(INPUT)); api.observe(&result.value.1);",
        ),
    ];

    let mut failures = Vec::new();

    for (name, body) in cases {
        for valid in [true, false] {
            let body = body.replace("INPUT", if valid { "api.make()" } else { "none" });

            if let Some(failure) = check(&provider, imported, name, &body, valid) {
                failures.push(failure);
            }
        }
    }

    for (name, body, valid) in [
        ("unchanged_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.keep(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);", true),
        ("changed_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);", false),
        ("independent_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_independent(&result.1);", true),

        (
            "projected_input",
            "let result = api.extract<(api.Receipt?)>(api.product<(api.Receipt?)>(api.make())); api.observe(&result.1);",
            true,
        ),
        (
            "two_inputs",
            "let result = api.pair<(api.Receipt?)>(api.make(), api.make()); api.observe(&result.0); api.observe(&result.1);",
            true,
        ),
        (
            "two_inputs_forged",
            "let result = api.pair<(api.Receipt?)>(api.make(), none); api.observe(&result.1);",
            false,
        ),
        (
            "copied_input",
            "let original = api.stamp(); let result = api.tuple<api.Stamp>(original); api.observe_stamp(&result.1);",
            false,
        ),
        (
            "copied_product",
            "let original = api.stamp(); let result = api.product<api.Stamp>(original); api.observe_stamp(&result.value);",
            false,
        ),
        (
            "live_address",
            "let payload: u32 = 4; let result = api.tuple<api.Address>(api.address(&payload)); api.observe_address(&result.1);",
            true,
        ),
        (
            "expired_address",
            "let result = { let payload: u32 = 4; yield api.tuple<api.Address>(api.address(&payload)); }; api.observe_address(&result.1);",
            false,
        ),
        (
            "independent_marker",
            "let result = { let payload: u32 = 4; yield api.tuple<api.Address>(api.address(&payload)); }; api.observe_marker(&result.1);",
            true,
        ),
        (
            "live_address_mutation",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_address(&result.1);",
            true,
        ),
        (
            "expired_address_mutation",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_address(&result.1);",
            false,
        ),
        (
            "marker_mutation",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_marker(&result.1);",
            true,
        ),
        (
            "expired_reference",
            "let result = { let payload: u32 = 4; yield api.with_reference<(api.Receipt?)>(api.make(), &payload); }; api.observe(&result.0); api.reference(result.1);",
            false,
        ),
    ] {
        if let Some(failure) = check(&provider, imported, name, body, valid) {
            failures.push(failure);
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check(
    provider: &crate::Compilation,
    imported: bool,
    name: &str,
    body: &str,
    valid: bool,
) -> Option<String> {
    let effect = if body.contains("await ") {
        "async "
    } else {
        ""
    };

    let source =
        format!("module app; using api; {effect}func caller(pos choice: bool) {{ {body} }}");

    let consumer = if imported {
        execution_consumer(
            provider,
            &source
                .replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."),
        )
    } else {
        compilation_from_sources([API, &source])
    };

    let diagnostics = consumer.check_diagnostics();

    if diagnostics.has_errors() == valid {
        return Some(format!(
            "{name}, imported={imported}, valid={valid}: {diagnostics:#?}"
        ));
    }

    if !valid {
        if !diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                    | bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency
                    | bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
            )
        }) {
            return Some(format!("unexpected rejection for {name}: {diagnostics:#?}"));
        }
    }

    None
}

#[test]
fn source_owned_transfers_preserve_complete_carriers_and_dependencies() {
    check_owned_transfers(false);
}

#[test]
fn imported_owned_transfers_preserve_complete_carriers_and_dependencies() {
    check_owned_transfers(true);
}
