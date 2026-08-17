# Patterns

**Specification:** [Patterns](https://github.com/lejmer/bray/blob/develop/docs/language/patterns.md)

## Contents

- [Pattern forms](#pattern-forms)
- [Choose the pattern by intent](#choose-the-pattern-by-intent)

## Pattern forms

**Core model:** A pattern structurally checks a value, may bind or access its parts, and establishes refinements. Its context decides whether failure is allowed and whether matched storage is observed, borrowed, copied, or consumed.

The following independent fragments assume the named products, unions, constants, and helper callables have the fields and signatures implied by their use.

### Bindings and irrefutable destructuring

```bray
module pattern_example;

func destructure(
    pos point: Point,
    pos user: User,
    pos singleton: (i32,),
    pos pair: (i32, string),
    pos exact: [i32; 3],
    pos leading: [i32; 4],
    pos edges: [i32; 4],
    pos boxed_message: box Message,
)
{
    let _ = unit;
    let value: i32 = 1;
    let mut counter: i32 = 0;
    let (grouped): i32 = value;

    counter += 1;

    let (only,) = singleton;
    let (number, text) = pair;

    let Point { x, y = vertical } = point;
    let { id, name, .. }: User = user;

    let [first, second, third] = exact;
    let [head, ..] = leading;
    let [start, .., end] = edges;

    let box(message) = boxed_message;

    inspect(
        counter,
        only,
        number,
        text,
        x,
        vertical,
        id,
        name,
        first,
        second,
        third,
        head,
        start,
        end,
        message,
    );
}
```

### Literals, constants, paths, and name resolution

```bray
func literal_patterns(
    integer: i32,
    real: r64,
    imaginary: c128,
    flag: bool,
    character: char,
    text: string,
)
{
    match integer
    {
        case 0 { record("zero"); }
        case Limits.DEFAULT { record("default"); }
        case value { record_integer(value); }
    }

    match real
    {
        case 0.0 { record("zero real"); }
        case _ { record("other real"); }
    }

    match imaginary
    {
        case 2.0i { record("imaginary"); }
        case _ { record("other imaginary"); }
    }

    match flag
    {
        case true { record("true"); }
        case false { record("false"); }
    }

    match character
    {
        case 'a' { record("letter a"); }
        case _ { record("other character"); }
    }

    match text
    {
        case "" { record("empty"); }
        case _ { record("nonempty"); }
    }
}

func status_pattern(pos status: Status)
{
    match status
    {
        case Status.Ready { record("qualified variant"); }
        case Waiting { record("unqualified variant"); }
        case other { record_status(other); }
    }
}
```

[Pattern resolution](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/pattern-resolution.md) first treats a bare name as an in-scope constant or union variant that is valid as a pattern. It introduces a new binding only when no such declaration resolves. The `_` pattern always discards without binding.

### Products and union variants

```bray
func describe_event(pos event: Event) -> string
{
    return match event
    {
        case Event.Idle { yield "idle"; }
        case Data(header, payload = body, ..)
        {
            inspect(header, body);

            yield "data";
        }
        case .Failed(code = code, ..) { yield describe_failure(code); }
    };
}

func product_patterns(pos point: Point, pos user: User)
{
    match point
    {
        case Point { x = 0.0, y = 0.0 } { record("origin"); }
        case Point { x, y } { record_point(x, y); }
    }

    match user
    {
        case { id, name = "root", .. } { record_root(id); }
        case { id, .. } { record_user(id); }
    }
}
```

Variant payload entries may be positional only for `pos` fields. Positional entries precede named entries, named fields may appear in any order, shorthand `field` means `field = field`, and `..` explicitly accepts unlisted fields without binding them.

### Nullable, boxed, and alternative patterns

```bray
func inspect_optional(pos user: User?)
{
    match user
    {
        case ?{ id, name, .. } { record_user(id, name); }
        case none { record("missing"); }
    }
}

func inspect_box(pos envelope: box Envelope)
{
    match envelope
    {
        case box(Envelope { header, .. }) { record_header(header); }
    }
}

func inspect_lookup(pos lookup: Lookup)
{
    match lookup
    {
        case Loaded(value) | Cached(value) { inspect(value); }
        case Missing | Failed { record("unavailable"); }
    }
}
```

Every alternative must bind the same names with compatible types, access modes, lifetimes, and capability requirements so the arm body has one stable environment.

### Pattern contexts and operation modes

```bray
func context_patterns(
    pos shared_points: [Point; 4],
    pos mut mutable_points: [Point; 4],
    pos owned_points: [Point; 4],
    path: Path,
)
{
    let Point { x, y } = Point.origin();

    with (file, metadata) = open_with_metadata(path)
    {
        inspect(file, metadata);
    }

    for Point { x, y } in shared_points
    {
        inspect(x, y);
    }

    for point in mut mutable_points
    {
        update(point);
    }

    for point in move owned_points
    {
        consume(point);
    }

    let coordinates =
    [
        each Point { x, y } in shared_points
        {
            yield (x, y);
        }
    ];

    inspect(x, y, coordinates);
}
```

[Local declarations, `with`, `for`, and generator binders](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/refutability.md) require irrefutable patterns. A `match` accepts refutable patterns. The enclosing construct selects the observe, shared-borrow, mutable-borrow, copy, or consume [operation mode](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/pattern-operation-modes.md), not punctuation inside the pattern.

### Guards, consuming matches, and partial moves

```bray
func take_payload(pos envelope: Envelope) -> Payload
{
    let Envelope { payload, .. } = envelope;

    return payload;
}

func consume_message(pos message: Message) -> Payload
{
    return match consume message
    {
        case Data(payload, ..) when payload.is_ready()
        {
            yield payload;
        }
        case Data(_, ..) | Empty
        {
            yield Payload.empty();
        }
    };
}
```

Structural matching runs before a guard. A guard may only observe its pattern bindings and subject. It cannot mutate, consume, suspend, transfer control, or perform another effect. Moving selected fields from an owned subject leaves the remainder partially initialized and destroys only the fields still initialized when the scope exits.

## Choose the pattern by intent

| Intent                          | Pattern form                                                                                                                                   | Decisive rule                                                                                                         |
|---------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| Ignore one value                | [`_`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/core-pattern-forms.md)                                                | Discard matches anything and introduces no name.                                                                      |
| Bind the whole matched value    | [`name` or `mut name`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/binding-patterns.md)                                 | A bare name binds only after pattern-capable declarations fail to resolve. A `mut` applies to an owned binding.       |
| Match a known value             | [Literal, constant, or path pattern](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/literal-patterns.md)                   | Matching uses structural value identity and never calls user-defined equality.                                        |
| Inspect a product               | [`Type { ... }` or expected-type `{ ... }`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/product-patterns.md)            | List every field or use `..`. Field order is irrelevant and shorthand binds the same spelling.                        |
| Inspect a union value           | [Qualified, unqualified, or leading-dot variant](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/union-variant-patterns.md) | The subject type must resolve the variant, and payload entries follow the declared positional/named field modes.      |
| Split tuples or arrays          | [Tuple or array pattern](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/tuple-and-array-patterns.md)                       | Tuple arity is exact. An array `..` accepts an unbound middle remainder.                                              |
| Distinguish nullable states     | [`none` or `?pattern`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/nullable-patterns.md)                                | `?pattern` matches the present value and recursively checks its payload.                                              |
| Destructure owned boxed storage | [`box(pattern)`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/box-patterns.md)                                           | The nested pattern applies to the box contents under the context's operation mode.                                    |
| Share one arm across shapes     | [An alternative pattern with two branches](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/alternative-patterns.md)         | All alternatives must establish the same binding environment.                                                         |
| Add an observed condition       | [`case pattern when condition`](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/guards.md)                                  | Guards exist only on match arms, run after structural success, and count toward exhaustiveness only when proven true. |
| Move selected components        | A pattern in a [consuming context](https://github.com/lejmer/bray/blob/develop/docs/language/patterns/partial-moves-through-patterns.md)       | Partial moves require ownership and preserve destruction obligations for components that remain initialized.          |

**Remember:** Patterns are structural and effect-free, contexts determine refutability and access mode, and `mut name` makes a new owned binding mutable rather than mutating the subject. Successful matches refine variants, literals, initialization state, shape, nullability, and box contents.
