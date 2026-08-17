# Callables

**Specification:** [Callables](https://github.com/lejmer/bray/blob/develop/docs/language/callables.md)

## Contents

- [Callable surfaces](#callable-surfaces)
- [Choose the callable by intent](#choose-the-callable-by-intent)

## Callable surfaces

**Core model:** A callable contract includes parameter names and call positions, types, ownership and borrowing, execution mode, ABI, conditions, capabilities, and result behavior. Every call and callable value must preserve that complete surface.

The following independent fragments assume referenced support types, predicates, capabilities, and helper callables are in scope. Bodies retain required result exits but abbreviate behavior that does not affect the callable surface.

### Functions, parameters, defaults, and calls

Parameters are named by default. An initial `pos` run permits positional arguments, and callers may still name those parameters. `mut` before a name makes an owned parameter binding mutable, and borrow types express shared or exclusive access:

```bray
module callable_example;

func analyze(
    pos source: &DataSet,
    pos mut workspace: Workspace,
    output: &mut Report,
    limit: usize = DEFAULT_LIMIT,
    mode: AnalysisMode = default_mode(limit),
) -> Result<Model, AnalysisError>
    requires(limit > 0)
    ensures(result.is_ok() || output.has_errors())
{
    workspace.prepare();
    output.record(source);

    return build_model(source, workspace, limit, mode);
}

func parameter_forms(
    pos owned: Packet,
    pos mut mutable_owned: Buffer,
    shared: &Packet,
    exclusive: &mut Buffer,
    nested_shared: &&Packet,
    nested_exclusive: &mut &mut Buffer,
) -> Packet
{
    mutable_owned.clear();
    exclusive.clear();

    observe(shared, nested_shared);
    update_nested(nested_exclusive);

    return owned;
}

func call_forms(
    pos source: DataSet,
    pos first_workspace: Workspace,
    pos second_workspace: Workspace,
    pos mut first_report: Report,
    pos mut second_report: Report,
)
{
    let defaulted: Result<Model, AnalysisError> = analyze(
        &source,
        first_workspace,
        output = &mut first_report,
    );

    let explicit: Result<Model, AnalysisError> = analyze(
        source = &source,
        workspace = second_workspace,
        limit = 32,
        mode = AnalysisMode.Thorough,
        output = &mut second_report,
    );

    inspect(defaulted, explicit);
}
```

Explicit arguments evaluate in source order. Omitted defaults then evaluate in parameter declaration order, and supplying an explicit argument suppresses only that default's evaluation.

### Execution modes, contracts, trust, visibility, and ABI

```bray
module callable_example;

const func clamp(pos value: i32, minimum: i32, maximum: i32) -> i32
    requires(minimum <= maximum)
    ensures(result >= minimum, result <= maximum)
{
    if value < minimum
    {
        return minimum;
    }

    if value > maximum
    {
        return maximum;
    }

    return value;
}

async func fetch(pos request: Request) -> Result<Response, FetchError>
    requires(network_execution())
{
    return await send(request);
}

trusted func read_byte(pos pointer: RawPointer<u8>) -> u8
    uses(raw_memory)
{
    return trusted core.memory.read<u8>(pointer);
}

@abi(c)
func compare(pos left: i32, pos right: i32) -> i32
{
    if left < right
    {
        return -1;
    }

    if left > right
    {
        return 1;
    }

    return 0;
}

@link(name = "native")
@symbol(name = "native_clock")
@abi(c)
extern trusted func native_clock() -> u64
    uses(foreign_call);

@symbol(name = "native_printf")
@abi(c)
extern trusted func printf(pos format: RawPointer<std.ffi.c.char>, ...) -> std.ffi.c.int
    uses(foreign_call);

internal func normalize(pos value: i32) -> i32
{
    return value;
}

async func execution_forms(pos request: Request, pos pointer: RawPointer<u8>) -> Result<unit, FetchError>
{
    let pending: Future<Result<Response, FetchError>> = fetch(request);
    let response: Response = try await pending;
    let byte: u8 = trusted read_byte(pointer);
    let normalized: i32 = internal callable_example.normalize(1);

    inspect(response, byte, normalized);

    return Ok(unit);
}
```

An async call produces an inactive `Future<T>` whose execution obligations remain attached to it. `const` exposes constant-evaluation eligibility, while `trusted` and `uses(...)` describe exact trusted declaration and implementation capability boundaries. Variadic syntax follows at least one fixed parameter on an extern trusted function or ABI-qualified callable type, with positional trailing arguments and the selected ABI's promotions. Callable declarations are public by default. External use of [`internal`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/visibility-and-paths.md) behavior requires path-specific acknowledgement. `uses(...)` lists trusted capabilities, not ordinary safe mutation, allocation, or I/O.

### Generics and explicit overloads

```bray
module callable_example;

func element_at<T, const N: usize>(pos values: &Buffer<T, N>, index: usize) -> &T
    with(N > 0)
    requires(index < N)
{
    return reference_at(values, index);
}

func parse_integer(pos text: string, radix: i32 = 10) -> i64
{
    return parse_i64(text, radix);
}

func parse_real(pos text: string) -> r64
{
    return parse_r64(text);
}

overload parse =
{
    parse_integer,
    parse_real,
}

func selection_forms(pos values: &Buffer<i32, 4>)
{
    let element: &i32 = element_at<i32, 4>(values, index = 1);
    let integer: i64 = parse("ff", radix = 16);
    let real: r64 = parse("1.5");
    let direct: i64 = parse_integer("10");

    inspect(element, integer, real, direct);
}
```

Generic callable arguments are explicit. Overload selection uses the explicitly supplied argument mapping, compatible types, receiver mode, explicit generic substitution, static constraints, and target availability. It does not use defaults or expected result type and never ranks candidates.

### Callable types, named contracts, and higher-order calls

```bray
module callable_example;

callable Mapper<T, U> = func(pos value: T) -> U;

callable CheckedMapper<T, U>
    with(T: Copyable) = func(pos value: T) -> U
    requires(valid(value))
    ensures(valid_result(result));

callable FixedConsumer<T, const N: usize> = func(pos values: &[T; N]);

func apply<T, U>(pos value: T, operation: Mapper<T, U>) -> U
{
    return operation(value);
}

func callable_values(pos request: Request, pos pointer: RawPointer<u8>)
{
    let ordinary: func(pos value: i32) -> i32 = double;
    let constant: const func(pos value: i32) -> i32 = constant_double;
    let asynchronous: async func(pos request: Request) -> Result<Response, FetchError> = fetch;

    let trusted_reader: trusted func(pos pointer: RawPointer<u8>) -> u8
        uses(raw_memory) = read_byte;

    let callback: @abi(c) func(pos left: i32, pos right: i32) -> i32 = compare;

    let doubled: i32 = apply<i32, i32>(2, operation = ordinary);
    let byte: u8 = trusted trusted_reader(pointer);

    inspect(constant, asynchronous, callback, doubled, byte, request);
}
```

A named `callable` declaration names one reusable callable type and creates no function or wrapper value. Callable values preserve parameter call positions, execution mode, ABI, contracts, capabilities, ownership, and result behavior.

### Static functions and receiver modes

```bray
module callable_example;

struct Counter
{
    mut value: i32;
}

impl Counter
{
    static func zero() -> Self
    {
        return { value = 0 };
    }

    func current() -> i32
    {
        return self.value;
    }

    mut func increment(amount: i32 = 1)
    {
        self.value += amount;
    }

    consume func into_value() -> i32
    {
        return consume self.value;
    }

    consume mut func reset() -> Self
    {
        self.value = 0;

        return consume self;
    }

    mut async func persist() -> Result<unit, StorageError>
    {
        return await persist_value(self.value);
    }

    trusted mut func overwrite(pos pointer: RawPointer<i32>)
        uses(raw_memory)
    {
        trusted core.memory.write<i32>(pointer, self.value);
    }
}

func receiver_forms(pos mut counter: Counter) -> i32
{
    let zero: Counter = Counter.zero();
    let current: i32 = counter.current();

    counter.increment(amount = 2);

    let reset: Counter = counter.reset();
    let value: i32 = reset.into_value();

    inspect(zero, current);

    return value;
}
```

`func`, `mut func`, `consume func`, and `consume mut func` select shared, mutable, consuming, and consuming-mutable receivers. The receiver is supplied by method-call syntax and appears as `self` only inside the body. A `static func` has no receiver.

### Local callable values and return scopes

```bray
module callable_example;

func local_callables() -> i32
{
    let increment = lambda(pos value: i32) -> i32
    {
        return value + 1;
    };

    let positive = const lambda(pos value: i32) -> i32
        requires(value >= 0)
        ensures(result >= 0)
    {
        return value;
    };

    let background = async lambda(pos request: Request) -> Result<Response, FetchError>
    {
        return await send(request);
    };

    let raw = trusted lambda(pos pointer: RawPointer<u8>) -> u8
        uses(raw_memory)
    {
        return core.memory.read<u8>(pointer);
    };

    let callback: @abi(c) func(pos value: i32) -> i32 =
        @abi(c)
        lambda(pos value: i32) -> i32
        {
            return value;
        };

    inspect(positive, background, raw, callback);

    return increment(1);
}

func log(pos message: string)
{
    print(message);
}

func fail(pos message: string) -> never
{
    panic(message);
}
```

Lambdas are capture-free callable values with their own execution scope, so their `return` exits the lambda rather than an enclosing callable. Pass a receiver explicitly when local callable behavior needs one. A `yield` supplies only the nearest yield-capable expression region. A `unit` callable may complete normally. Every normal path from a non-`unit` callable must return a compatible value, while a `never` callable has no normal completion.

## Choose the callable by intent

| Intent                                  | Bray form                                                                                                                                        | Decisive rule                                                                                             |
|-----------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|
| Define module behavior                  | [A function declaration](https://github.com/lejmer/bray/blob/develop/docs/language/callables/function-declarations.md)                           | Omitted result type means `unit`. Every normal non-`unit` path returns explicitly.                        |
| Attach behavior to a type or trait      | [An instance method or `static func`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/methods.md)                            | Receiver modifiers select receiver authority. A static function has no receiver.                          |
| Permit positional arguments             | [An initial `pos` parameter run](https://github.com/lejmer/bray/blob/develop/docs/language/callables/parameters.md)                              | Parameters remain named bindings, and callers may still supply `pos` parameters by name.                  |
| Omit a routine argument                 | [A parameter default](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/defaulted-arguments.md)                              | Defaults run only after callable selection and all explicit arguments.                                    |
| Parameterize behavior                   | [Type and `const` generic parameters plus `with(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/generic-functions.md) | Calls always supply generic arguments explicitly.                                                         |
| Share one call name                     | [An explicit `overload`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/function-overloading.md)                            | Selection must find exactly one arm and ignores defaults, result type, and expected type.                 |
| Pass behavior as a value                | [A callable type](https://github.com/lejmer/bray/blob/develop/docs/language/callables/callable-types-and-values.md)                              | The target type must preserve every caller-visible obligation of the value.                               |
| Name a reusable callable contract       | [`callable`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/callable-types-and-values.md#named-callable-contracts)          | It names a callable type only and creates neither a value nor an adapter.                                 |
| Define block-local behavior             | [A lambda](https://github.com/lejmer/bray/blob/develop/docs/language/callables/lambda-expressions-and-anonymous-callables.md)                    | Lambdas create callable values but cannot capture locals, `self`, or scoped capabilities.                 |
| Permit compile-time evaluation          | [`const func`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/const-functions.md)                                           | The body must be deterministic, total for valid inputs, terminating, effect-free, and allocation-free.    |
| Define suspendable execution            | [`async func`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/async-functions-and-computations.md)                          | Calling creates an inactive `Future<T>`. Body effects and postconditions arise only through execution.    |
| Cross a trusted implementation boundary | [`trusted` with exact `uses(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/trusted-functions.md)                     | Trusted implementation capabilities and caller obligations are distinct parts of the contract.            |
| Cross a callable ABI boundary           | [`@abi(...)`, optionally with `extern`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/callable-abi-and-ffi.md)             | ABI is part of callable identity. An `extern` declaration receives its body from another linked artifact. |
| State caller and completion conditions  | [`requires(...)` and `ensures(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/contract-clauses-on-functions.md)       | `requires` constrains entry, while `ensures` can refer to the compiler-introduced `result`.               |

**Remember:** Parameters and generic arguments are explicit, callable values preserve the complete contract, async calls return inactive futures, receiver modes select method authority, and lambdas capture nothing.
