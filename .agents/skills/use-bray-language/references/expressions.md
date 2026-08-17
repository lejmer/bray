# Expressions

**Specification:** [Expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions.md)

## Contents

- [Expression forms](#expression-forms)
- [Choose the direct form](#choose-the-direct-form)

## Expression forms

**Core model:** Expressions compute typed values or access paths, perform effects, transfer control and ownership, and establish conditions available to later checks.

The following independent fragments assume referenced support types, traits, capabilities, and helper callables are in scope. Each fragment demonstrates a distinct part of the expression inventory.

### Values, access paths, calls, and construction

```bray
module expression_example;

func value_forms()
{
    let mut point: Point = sample_point();
    let mut items: [i32; 4] = [1, 2, 3, 4];
    let reader: Buffer = sample_buffer();
    let index: usize = 1;
    let start: usize = 1;
    let end: usize = 3;

    let integer: i32 = 1;
    let real: r64 = 1.0;
    let imaginary: c128 = 2.0i;
    let complex: c128 = 1.0 + 2.0i;
    let boolean: bool = true;
    let character: char = 'a';
    let text: string = "text";
    let empty: string = "";
    let done: unit = unit;
    let missing: i32? = none;

    let grouped: i32 = (integer + 1);
    let single = (integer,);
    let pair: (i32, r64) = (integer, real);
    let array: [i32; 3] = [1, 2, 3];
    let repeated: [i32; 4] = [0; 4];
    let indices: Range<i32> = 0..4;

    let name: i32 = integer;
    let qualified: r64 = math.pi;
    let acknowledged: usize = internal expression_example.detail.limit;
    let field: r64 = point.x;
    let tuple_element: i32 = pair.0;
    let element: i32 = items[index];

    items[index] = 5;

    let middle: &[i32] = &items[start..end];
    let suffix: &[i32] = &items[start..];
    let prefix: &[i32] = &items[..end];
    let whole: &[i32] = &items[..];

    let function_result: i32 = add(integer, right = 2);
    let defaulted_result: Retry = retry();
    let generic_result: i32 = identity<i32>(integer);
    let method_result: usize = items.count();
    let static_result: Point = Point.origin();
    let qualified_result: i32 = reader(Reader<Bytes>).read_next();

    let full: Point = Point
    {
        x = 1.0,
        y = 2.0,
    };

    let expected: Point =
    {
        x = 3.0,
        y = 4.0,
    };

    let full_variant: Shape = Shape.Circle(center = sample_point(), radius = 1.0);
    let dotted_variant: Shape = .Circle(center = sample_point(), radius = 2.0);
    let contextual_variant: Shape = Circle(center = sample_point(), radius = 3.0);
    let full_empty_variant: Shape = Shape.Empty;
    let dotted_empty_variant: Shape = .Empty;
    let contextual_empty_variant: Shape = Empty;

    let default_box: box Point = box(full);
    let heap_box: box[Heap] Point = box[Heap](expected);

    observe(&point);
    update(&mut point);
    point.x = 5.0;
    (point).x = 6.0;

    let widened: i64 = integer as i64;

    assert(index < items.count());
    assert(start <= end, "invalid slice bounds");
}

impl Counter
{
    mut func increment()
    {
        self.value += 1;
    }
}
```

Unconstrained integer, real, and complex literals default to `i32`, `r64`, and `c128`. An imaginary literal uses a trailing `i`. Context selects `c64` with `r32` components or `c128` with `r64` components. Literal components adapt directly, while already typed real and imaginary components form a complex value through explicit tuple conversion. A bounded [`start..end`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/range-expressions.md) expression produces an ascending half-open `Range<T>`.

Plain [`as`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/conversion-expressions.md) is only for total, value-preserving conversion. Use an explicit fallible conversion operation otherwise.

### Operators and assignment

```bray
predicate inside(value: i32, minimum: i32, maximum: i32) =
    value >= minimum && value <= maximum;

func operator_forms(
    pos mut scalar: i32,
    pos mut bits: u32,
    pos mut matrix: Matrix,
    other: Matrix,
)
{
    let negative: i32 = -scalar;
    let inverted: u32 = ~bits;
    let negated: bool = !false;

    let logical: bool = true || false && true;
    let equal: bool = scalar == 0;
    let unequal: bool = scalar != 0;
    let ordered: bool = scalar < 10 && scalar <= 10 && scalar > -10 && scalar >= -10;

    let bitwise: u32 = (bits | 1) ^ (bits & 3);
    let shifted: u32 = (bits << 1) >> 1;
    let arithmetic: i32 = scalar + 2 - 1 * 4 / 2 % 3;
    let multiplied: Matrix = sample_matrix() @ sample_matrix();
    let powered: i32 = scalar ** 2;

    scalar = 1;
    scalar += 1;
    scalar -= 1;
    scalar *= 2;
    scalar /= 2;
    scalar %= 2;
    scalar **= 2;

    bits &= 0xff;
    bits |= 1;
    bits ^= 2;
    bits <<= 1;
    bits >>= 1;

    matrix @= other;
}
```

Conditions require `bool`. Combine individual [comparisons](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/unary-and-binary-expressions.md) explicitly with short-circuiting `&&` or `||`.

### Generators and boolean folds

```bray
func generated_forms(
    pos groups: [[i32; 2]; 2],
    pos tested_values: [i32; 4],
)
{
    let doubled: [i32; 4] =
    [
        each value in 0..4
        {
            yield value * 2;
        }
    ];

    let flattened =
    {
        each group in move groups
        {
            each value in move group
            {
                yield value;
            }
        }
    };

    let every_positive: bool = all(
        {
            each value in move tested_values
            {
                yield value > 0;
            }
        }
    );

    let any_zero: bool = any([false, true, false]);
}
```

A fixed-array generator produces one element per iteration and a statically provable total of `N` elements. Constant ranges provide their exact half-open cardinality. General generators can produce a variable count according to their control flow.

### Blocks, conditions, matching, and loops

```bray
func control_forms(
    pos mut values: [i32; 4],
    pos shape: Shape,
    limit: i32,
) -> i32?
{
    let from_block: i32 =
    {
        log("block");
        yield limit;
    };

    let bounded: i32 = if from_block < 0
    {
        yield 0;
    }
    else if from_block > limit
    {
        yield limit;
    }
    else
    {
        yield from_block;
    };

    let radius: r64 = match shape
    {
        case Circle(center = _, radius = value) when value >= 0.0
        {
            yield value;
        }
        case Rectangle(..) | Empty
        {
            yield 0.0;
        }
    };

    let mut index: usize = 0;

    let from_while: i32? = while index < values.count()
    {
        let value: i32 = values[index];

        if value == bounded
        {
            break value;
        }

        index += 1;
    }
    else
    {
        break none;
    };

    for value in values
    {
        if value < 0
        {
            continue;
        }

        observe(value);
    }

    for value in mut values
    {
        update(value);
    }

    for value in move values
    {
        consume(value);
    }
    else
    {
        log("exhausted");
    }

    let from_loop: i32? = loop
    {
        if from_while != none
        {
            break from_while;
        }

        break none;
    };

    observe(radius);

    return from_loop;
}

func consuming_match(pos shape: Shape) -> r64
{
    return match consume shape
    {
        case Circle(center = _, radius = value)
        {
            yield value;
        }
        case Rectangle(..) | Empty
        {
            yield 0.0;
        }
    };
}
```

Matches over closed unions and nullable values receive coverage checking. Unguarded arms contribute their pattern regions, and guarded arms contribute the regions where the guard is statically proven true.

### Propagation, panic boundaries, trust, and async execution

```bray
func optional_name(pos id: UserId) -> string?
{
    let user: User = find_user(id)?;
    let profile: Profile = user.profile?;

    return profile.name;
}

func load_user(pos id: UserId) -> Result<User, LoadError>
{
    let row: Row = try fetch_row(id);
    let user: User = try decode_user(row);

    return Ok(user);
}

async func join_user(pos task: Task<Result<User, LoadError>>) -> Result<User, LoadError>
{
    let result: Result<User, LoadError> = try await task.join();
    let user: User = try result;

    return Ok(user);
}

func caught_user(pos input: string) -> Result<User, PanicReport>
{
    return catch
    {
        let user: User = parse_user(input);

        yield user;
    };
}

trusted func raw_sum(pos first: RawPointer<u8>, pos second: RawPointer<u8>) -> u8
    uses(raw_memory)
{
    let direct: u8 = trusted core.memory.read<u8>(first);

    let total: u8 = trusted
    {
        let other: u8 = core.memory.read<u8>(second);

        yield direct + other;
    };

    return total;
}

func require_nonzero(pos value: i32) -> i32
{
    if value == 0
    {
        panic("zero is not accepted");
    }

    return value;
}
```

### Scoped use and callable expressions

```bray
func callable_forms(pos path: Path) -> usize
{
    let lines: usize = with file: File = File.open(path)
    {
        yield file.count_lines();
    };

    with (file, metadata) = open_with_metadata(path)
    {
        inspect(file, metadata);
    }

    let increment = lambda(pos value: usize) -> usize
    {
        return value + 1;
    };

    let callback: @abi(c) func(pos value: i32) -> i32 =
        @abi(c)
        lambda(pos value: i32) -> i32
        {
            return value;
        };

    let background = async lambda() -> Response
    {
        return await read_response();
    };

    let raw = trusted lambda(pos pointer: RawPointer<u8>) -> u8
        uses(raw_memory)
    {
        return core.memory.read<u8>(pointer);
    };

    return increment(lines);
}

func unit_exits()
{
    let empty: unit = {};

    let value: unit =
    {
        yield;
    };

    let mut remaining: usize = 1;

    while remaining > 0
    {
        remaining -= 1;
    }

    loop
    {
        break;
    }

    return;
}
```

## Choose the direct form

| Intent                                    | Bray form                                                                                                                                                                                                                                                                                                                           | Decisive rule                                                                                                                                  |
|-------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------|
| Produce a value conditionally             | [`if`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/conditional-expressions.md) or [`match`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/match-expressions.md) with `yield`                                                                                                      | Every reachable normal path in a non-`unit` region yields a compatible value.                                                                  |
| Search while iterating                    | [`for`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/for-expressions.md), [`while`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/while-expressions.md), or [`loop`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/loop-expressions.md) with `break value` | `for` and `while` may use `else` for natural exhaustion. A `loop` has no exhaustion path.                                                      |
| Transform an iterable                     | A [general](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/general-generator-expressions.md) or [fixed-array](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/array-generator-expressions.md) generator                                                                                | `each` iterates and `yield` emits to the nearest generator region.                                                                             |
| Reduce booleans                           | [`all(...)` or `any(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/boolean-fold-expressions.md)                                                                                                                                                                                                       | Both forms short-circuit and accept finite bounded boolean iterables.                                                                          |
| Update an assignable path                 | [`=` or compound assignment](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/assignment-expressions.md)                                                                                                                                                                                                       | Compound assignment evaluates the destination once and returns `unit`.                                                                         |
| Propagate absence or failure              | [Postfix `?`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/nullable-and-absence-expressions.md) or [prefix `try`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/result-and-run-result-propagation-expressions.md)                                                                  | Each operator unwraps exactly one compatible layer and exits the nearest matching boundary otherwise.                                          |
| Suspend for an async result               | [`await`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/await-expressions.md)                                                                                                                                                                                                                               | Awaiting drives one async computation in the current async execution context and does not block a native thread by definition.                 |
| Isolate panic propagation                 | [`catch`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/catch-expressions.md) and [`panic(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/panic-expressions.md)                                                                                                                | `catch` converts a panic crossing its boundary into `Result<_, PanicReport>` while preserving ordinary result and nullable propagation.        |
| Enter and leave scoped lifecycle behavior | [`with`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/with-expressions.md)                                                                                                                                                                                                                                 | The initializer establishes a scoped capability, and every exit runs the matching lifecycle exit behavior.                                     |
| Define local callable behavior            | [`lambda`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/lambda-expressions-and-anonymous-callable-expressions.md)                                                                                                                                                                                          | Lambdas are capture-free callable values with the same parameter, result, contract, ABI, async, and trusted surfaces that their form declares. |
| Construct with known context              | [Expected-type construction](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/expression-context.md)                                                                                                                                                                                                           | Context may guide literals, variants, structs, arrays, tuples, and boxes, but does not select overloads.                                       |
| Call an operation                         | [Named arguments, with a positional `pos` prefix](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/arguments.md)                                                                                                                                                                                               | Binding is by parameter identity, while runtime evaluation remains caller source order.                                                        |

**Remember:** [Block-shaped expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/sequenced-expressions.md) are self-delimiting, while other sequenced expressions require semicolons. A `yield` supplies the nearest value-producing region, a `return` exits the callable, and a `break` supplies a loop result. A `try` unwraps one `Result` or `RunResult` layer. Evaluation otherwise follows source order.
