# Declarations

**Specification:** [Declarations](https://github.com/lejmer/bray/blob/develop/docs/language/declarations.md)

## Contents

- [Declaration surfaces](#declaration-surfaces)
- [Choose the declaration by intent](#choose-the-declaration-by-intent)

## Declaration surfaces

**Core model:** Every declaration is checked in its declaration context. Module, type, trait, implementation, and block contexts deliberately accept different declaration forms, members, bodies, and modifiers. [Source order](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/declaration-order-and-checking.md) does not affect identity or lookup.

Give every visible declaration, generic parameter, callable parameter, pattern binding, and local binding a fresh name while another [ordinary name](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/declaration-names-and-identity.md) with that spelling is visible.

[Declaration-owned expressions](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/declaration-owned-expressions.md) are checked with their declarations. Runtime defaults evaluate when omitted, and an explicit value selects the supplied expression while declaration checking still validates the default. Parameter defaults can use the receiver and earlier parameters. Each struct field and union payload default is independent of the represented object and sibling fields. Static initializers are constant templates materialized for each demanded closed instance.

The following independent fragments assume referenced support types, traits, capabilities, and helper callables are in scope. Bodies retain required result exits but abbreviate behavior that does not affect the declaration surface.

### Module contributions and module body items

A source file starts with either one source-unit module declaration or one or more block module declarations. A source-unit module
can own an unbraced production prefix followed by braced module contributions:

```bray
@target(target.atomic.U64)
module declaration_example;

using std.convert;
using internal declaration_example.detail;
export parse;
```

```bray
@link(name = "native")
trusted internal module declaration_example.ffi
{
    @symbol(name = "native_clock")
    @abi(c)
    extern trusted func native_clock() -> u64
        uses(foreign_call);
}
```

```bray
@test
module declaration_example.tests
{
    using declaration_example;

    @test
    func parses_default_input()
    {
        // ...
    }
}
```

```bray
module declaration_example.application
{
    @entrypoint
    func main()
    {
        // ...
    }
}
```

### Module-level declarations

This fragment covers every module-level declaration form:

```bray
module declaration_example;

const DEFAULT_LIMIT: usize = 10;

static PROCESS_TOTAL: usize = 0;

@thread_local
static THREAD_TOTAL: usize = 0;

@link(name = "native")
@symbol(name = "native_counter")
extern trusted static mut NATIVE_COUNTER: std.ffi.c.UnsignedInt;

static EMPTY_BUFFER<T, const N: usize>: Buffer<T, N>
    with(T: Copyable) = Buffer<T, N>.empty();

predicate within(value: usize, limit: usize) =
    value <= limit;

trusted predicate valid_address<T>(pointer: RawPointer<T>);

callable Mapper<T, U> =
    func(pos value: T) -> U;

callable OrderedPredicate<T>
    with(T: Comparable<T>) =
    func(pos left: &T, pos right: &T) -> bool;

func identity<T>(pos value: T) -> T
{
    return value;
}

const func clamp(pos value: usize, limit: usize = DEFAULT_LIMIT) -> usize
    ensures(result <= limit)
{
    if value > limit
    {
        return limit;
    }

    return value;
}

async func load_count(pos key: u64) -> usize
{
    return read_count(key);
}

trusted func pointer_at<T>(address: usize) -> RawPointer<T>
    uses(raw_memory)
{
    return pointer_from_address<T>(address);
}

@link(name = "c")
@symbol(name = "getpid")
@abi(c)
extern trusted func process_id() -> i32
    uses(foreign_call);

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

@copy
@layout(c)
struct Header
{
    magic: u32;
    internal mut attempts: u16 = 0;
}

@layout(stable, tag = u8)
union Packet<T>
{
    @tag(0)
    Empty;

    @tag(1)
    Data(pos value: T, mut retries: usize = 0);
}

trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}

impl Header(Equatable<Header>)
{
    func equals(pos other: &Header) -> bool
    {
        return self.magic == other.magic;
    }
}
```

An `@thread_local static` has one demand-initialized instance per exact native-thread attachment. Its borrows carry that
attachment identity, and its cleanup runs on the same thread when the attachment ends.

### Type members and lifecycle declarations

Type bodies accept fields or variants plus callable members, constructors, lifecycle declarations, associated constants and predicates, and callable overloads:

```bray
struct ValueBox<T, const N: usize>
    with(T: Copyable)
{
    value: T;
    internal mut updates: usize = 0;

    const CAPACITY: usize = N;

    predicate has_room(count: usize) =
        count < N;

    construct(pos value: T) -> Self
    {
        return
        {
            value = value,
            updates = 0,
        };
    }

    construct counted(pos value: T, updates: usize) -> Self
    {
        return
        {
            value = value,
            updates = updates,
        };
    }

    static func capacity() -> usize
    {
        return N;
    }

    func current() -> &T
    {
        return &self.value;
    }

    mut func record_update()
    {
        self.updates += 1;
    }

    consume func into_value() -> T
    {
        return consume self.value;
    }

    consume mut func normalized() -> Self
    {
        self.updates = 0;

        return consume self;
    }

    func format_compact() -> string
    {
        return compact_text(&self.value);
    }

    func format_with_width(width: usize) -> string
    {
        return detailed_text(&self.value, width);
    }

    overload format =
    {
        format_compact,
        format_detailed,
    }
}

struct Resource
{
    handle: u64;
}

impl Resource
{
    type Handle = u64;

    const INVALID_HANDLE: Handle = 0;

    predicate open(value: &Self) =
        value.handle != INVALID_HANDLE;

    construct(pos handle: Handle) -> Self
        requires(handle != INVALID_HANDLE)
    {
        return { handle = handle };
    }

    construct closed() -> Self
    {
        return { handle = INVALID_HANDLE };
    }

    async finalize() -> Result<unit, CleanupError>
    {
        return finalize_handle(self.handle);
    }

    destruct()
    {
        destroy_handle(self.handle);
    }

    enter() -> Lease
    {
        return acquire_lease(self);
    }

    exit(pos lease: Lease)
    {
        release_lease(lease);
    }
}
```

### Trait members and implementation declarations

Trait bodies distinguish required members from defaulted members. Implementation headers distinguish inherent, unnamed trait, and named trait implementations:

```bray
trait Collection<T>
{
    type Item;

    const CAPACITY: usize;
    const RETRIES: usize = 3;

    predicate valid(value: &Self);

    predicate accepts(value: &T) =
        true;

    func length() -> usize;

    func is_empty() -> bool
    {
        return self.length() == 0;
    }

    mut func push(pos value: T);
    consume func finish() -> Item;
    consume mut func normalize() -> Self;
    static func empty() -> Self;

    async finalize() -> Result<unit, CleanupError>;
    destruct();
    enter() -> Lease;
    exit(pos lease: Lease);
}

trait Inspect<Mode>
{
    type Output;

    const VERSION: usize;

    predicate ready(value: &Self);

    func inspect() -> Output;
}

impl ResourceFastInspect = Resource(Inspect<Fast>)
{
    type Output = u64;

    const VERSION: usize = 1;

    predicate ready(value: &Self) =
        value.handle != 0;

    func inspect() -> Output
    {
        return self.handle;
    }
}

impl ResourceSafeInspect = Resource(Inspect<Safe>)
{
    type Output = u64;

    const VERSION: usize = 1;

    predicate ready(value: &Self) =
        value.handle != 0;

    func inspect() -> Output
    {
        return checked_handle(self.handle);
    }
}

overload Resource(Inspect) =
{
    ResourceFastInspect,
    ResourceSafeInspect,
}
```

### Block-level declarations

Ordinary blocks admit local bindings and constants. Use a [lambda value](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/declaration-contexts.md) for local callable behavior:

```bray
func adjusted(pos value: usize) -> usize
{
    const LOCAL_LIMIT: usize = 4;

    let bounded: usize = clamp(value, limit = LOCAL_LIMIT);

    let increment = lambda(pos current: usize) -> usize
    {
        return current + 1;
    };

    return increment(bounded);
}
```

## Choose the declaration by intent

| Intent                                     | Bray declaration                                                                                                                                            | Decisive rule                                                                                                                    |
|--------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------|
| Introduce a module API                     | [A module-level declaration](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/module-body-declarations.md)                            | Module bodies accept functions, types, traits, implementations, predicates, overloads, constants, and statics.                   |
| Name a reusable callable contract          | [`callable`](https://github.com/lejmer/bray/blob/develop/docs/language/callables/callable-types-and-values.md)                                              | The declaration names a complete callable type surface and has no executable body.                                               |
| Define stored product or tagged union data | [`struct` or `union`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-declarations.md)                                                 | Product fields and union variants are representation declarations. Type bodies can also own behavior and lifecycle declarations. |
| Declare or fulfill behavior                | [`trait` or `impl`](https://github.com/lejmer/bray/blob/develop/docs/language/types/implementations.md)                                                     | Traits declare required or defaulted members. Implementations add inherent behavior or fulfill one trait application.            |
| Introduce type behavior or representation  | [A member declaration](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/member-declarations.md)                                       | Type, trait, and implementation bodies accept different member forms and ownership rules.                                        |
| Name a block-local value                   | [`let` or block-level `const`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/block-level-declarations.md)                          | Ordinary blocks do not contain named functions, types, traits, implementations, modules, or packages.                            |
| Limit ordinary reachability                | [`internal`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/visibility-and-reachability.md)                                         | External access remains possible through explicit, path-specific internal-use acknowledgement.                                   |
| Parameterize a declaration                 | [Type or `const` generics plus `with(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/generic-declarations-and-constraints.md) | Generic callable arguments are explicit. Implementation generics are inferred from unresolved header names instead.              |
| Name a compile-time value                  | [`const`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/constant-declarations.md)                                                  | Constants have no runtime storage identity and must be freely materializable.                                                    |
| Own persistent runtime storage             | [`static` or `@thread_local static`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/static-storage-declarations.md)                 | Statics have address-bearing instances, demand-driven materialization, and product-owned or exact-thread lifecycle ownership.    |
| Name provider-owned native storage         | [`extern static`](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/foreign-data-and-symbols.md)                  | A reference produces a provider-rooted raw pointer, with exact-thread attachment added for thread-local storage.                 |
| Name a contract relation                   | [`predicate`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/predicate-declarations.md)                                             | A predicate defines a checked relation, not one eagerly evaluated Boolean value.                                                 |
| Share one overloaded surface               | [`overload`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/overload-declarations.md)                                               | Same-name declarations never form an overload set automatically, and expected result types do not select arms.                   |
| Attach compile-time policy                 | [A directive](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/directives.md)                                                         | `@name` attaches only where that directive's contract permits and uses constant arguments.                                       |
| Change a declaration surface               | [A modifier](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/modifiers.md)                                                           | Modifier validity and meaning depend on the declaration form and context.                                                        |

**Remember:** Visibility-capable declarations are public by default, `internal` access requires lexical acknowledgement, generic callable arguments are explicit, and overload families are declared explicitly.
