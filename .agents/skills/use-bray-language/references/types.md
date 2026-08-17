# Types

**Specification:** [Types](https://github.com/lejmer/bray/blob/develop/docs/language/types.md)

## Contents

- [Type categories and forms](#type-categories-and-forms)
- [Named types, generics, and copying](#named-types-generics-and-copying)
- [Traits](#traits)
- [Implementations and member selection](#implementations-and-member-selection)
- [Operator contracts](#operator-contracts)
- [Choose the type form by intent](#choose-the-type-form-by-intent)
- [Common mistakes](#common-mistakes)

## Type categories and forms

**Core model:** A type is the complete semantic contract for a value or access path, including valid operations, representation, initialization, ownership, borrowing, mutation, destruction, finalization, capabilities, effects, and public compatibility.

The following independent fragments assume referenced support declarations and compiler-known types are in scope. Empty bodies intentionally keep the focus on the accepted type surfaces.

### Scalar and compiler-known named types

```bray
module type_example;

func scalar_types(
    signed: (i8, i16, i32, i64, i128),
    unsigned: (u8, u16, u32, u64, u128),
    machine: (isize, usize),
    real: (r32, r64),
    complex: (c64, c128),
    flag: bool,
    character: char,
    text: string,
    done: unit,
    pointer: RawPointer<u8>,
    result: Result<Value, Error>,
    run_result: RunResult<Value>,
    future: Future<Value>,
    task: Task<Value>,
)
{
}

func stop() -> never
{
    panic("stop");
}
```

Scalars with values are copyable and have no partial state. `never` has no values, `string` is a protected named value type rather than a scalar, and `RawPointer<T>`, result unions, futures, and tasks each follow their compiler-known contracts.

### Core type forms and composition

```bray
func type_forms(
    grouped: (Value),
    singleton: (Value,),
    tuple: (Value, string, usize),
    array: [Value; 4],
    shared: &Value,
    exclusive: &mut Value,
    nested: &&mut Value,
    optional: Value?,
    boxed: box Value,
    heap_boxed: box[Heap] Value,
    arena_boxed: box[ArenaStorage] Value,
    shared_view: &view Sink,
    exclusive_view: &mut view Sink,
    owned_view: box[Heap] view Sink,
    shared_slice: &[u8],
    exclusive_slice: &mut [u8],
    owned_slice: box[Heap] [u8],
    callback: func(pos value: i32) -> i32,
    compile_time: const func(pos value: i32) -> i32,
    asynchronous: async func(pos request: Request) -> Response,
    foreign: @abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32,
)
{
}

trusted func trusted_callable_type(
    callback: trusted func(pos pointer: RawPointer<u8>) -> u8
        uses(raw_memory),
)
    uses(raw_memory)
{
}

func composed_types(
    optional_box: (box[Heap] Value)?,
    boxed_optional: box[Heap] (Value?),
    borrowed_box: &mut box[Heap] Value,
    boxed_nodes: [box[Heap] Node; 4],
)
{
}
```

Grouping parentheses do not create a type. A one-element tuple requires its trailing comma. The default `box T` is identical to `box[Heap] T`, while another storage policy changes type identity. Unsized slices and trait views must appear behind a borrow or owned indirection.

## Named types, generics, and copying

### Products, fields, and generic identity

```bray
@copy
struct Point
{
    x: r64;
    y: r64;
}

@copy
struct FixedBuffer<T, const N: usize>
    with(T: Copyable)
{
    values: [T; N];
    internal mut cursor: usize = 0;
}

struct Counter
{
    mut value: i64;
}

func update_counter(pos mut counter: Counter)
{
    counter.value += 1;
}
```

A product field is immutable after initialization unless its declaration uses `mut`. Binding mutation authority and field mutability are separate. Type and const arguments are explicit, ordered, invariant parts of generic type identity. A `with(...)` clause constrains declared parameters and never introduces them.

### Closed unions, payloads, and recursive ownership

```bray
@copy
union Response<T>
    with(T: Copyable)
{
    Accepted(pos value: T);
    Retry(pos value: T, mut attempts: usize = 1)
        requires(attempts > 0);
    Rejected;
}

union List<T>
{
    Node(pos value: T, next: box Self);
    Empty;
}
```

A union has one active variant from a closed declared set. Payload fields are named, with `pos` granting positional construction and matching. A recursive stored ownership path must cross `box`, because borrows are non-owning and tuples, arrays, nullable forms, payloads, and generic applications remain inline.

### Copy contracts and lifecycle boundaries

`@copy` requests compiler-derived copying for a product or union representation. Every represented part of a concrete copyable instantiation must itself be copyable. Source code never implements `Copyable`, and copy behavior never runs user code or performs fallible or resource-duplicating work. A type with `@copy` cannot declare `finalize`, `destruct`, `enter`, or `exit` lifecycle behavior.

Named types retain declaration identity even when their representations match. Structural forms derive identity from their form, subject types, and compile-time arguments.

## Traits

### Trait inputs and every member category

```bray
trait Collection<Item, const LIMIT: usize>
{
    const CATEGORY: u8;
    const MAXIMUM: usize = LIMIT;

    type Cursor;

    predicate valid(value: &Self);

    predicate empty(value: &Self) =
        !valid(value);

    func count() -> usize;

    func is_empty() -> bool
    {
        return self.count() == 0;
    }

    mut func push(pos item: Item);

    consume func into_cursor() -> Cursor;

    consume mut func normalize() -> Self;

    static func create() -> Self;
}

trait Managed<Lease>
{
    finalize() -> Result<unit, CleanupError>;
    destruct();
    enter() -> Result<Lease, EnterError>;
    exit(scoped: Lease) -> unit;
}
```

Trait parameters are application inputs. Constant-valued and type-valued members are implementation-selected outputs. Callable and constant members may be required or defaulted, predicate members may be required or trait-defined, and lifecycle requirements use `finalize`, `destruct`, or paired `enter` and `exit` declarations.

### Trait views and static constraints

```bray
func first<I>(pos mut iterator: I) -> I(Iterator).Element?
    with(I: Iterator)
{
    return iterator.next();
}

func same_element<A, B>(left: A, right: B)
    with(
        A: Iterator,
        B: Iterator,
        A(Iterator).Element == B(Iterator).Element,
    )
{
}

func write_generic<S>(pos sink: S, pos message: string)
    with(S: Sink)
{
    sink.write(message);
}

func write_dynamic(pos sink: &view Sink, pos message: string)
{
    sink.write(message);
}

func selected_capacity() -> usize
{
    return PacketBuffer(HasCapacity).CAPACITY;
}

predicate selected_valid(value: &PacketBuffer) =
    PacketBuffer(BufferContract).valid(value);
```

Traits are compile-time behavioral contracts, not storable value types. Generic constraints retain the concrete type. A `view TraitApplication` hides it and dispatches through one implementation witness. Qualified members use `Subject(Trait).Member`, with grouped subjects such as `(&T)(Iterable).Element` for borrow implementations.

## Implementations and member selection

### Inherent, unnamed, named, generic, and borrow subjects

```bray
impl Sequence<T>
{
    type Cursor = SequenceCursor<T>;

    func count() -> usize
    {
        return self.length;
    }

    static func empty() -> Self
    {
        return Self.create();
    }
}

impl Point(Equatable<Point>)
{
    func equals(pos other: &Point) -> bool
    {
        return self.x == other.x && self.y == other.y;
    }
}

impl BorrowedSequenceIterable = &Sequence<T>(Iterable)
{
    type Element = &T;
    type Cursor = BorrowedSequenceCursor<T>;

    consume func iterate() -> Cursor
    {
        return BorrowedSequenceCursor<T>.from(self);
    }
}

impl MutableSequenceIterable = &mut Sequence<T>(Iterable)
{
    type Element = &mut T;
    type Cursor = MutableSequenceCursor<T>;

    consume func iterate() -> Cursor
    {
        return MutableSequenceCursor<T>.from(self);
    }
}

func accept_cursor(cursor: Sequence<u8>.Cursor)
{
}
```

An inherent implementation can be declared only by the named type's owning package. Trait implementations can target a named type, an inferred generic subject, `&T`, or `&mut T`. Generic implementation parameters are inferred from the subject and trait application, never from `with(...)` alone.

### Exact trait applications and implementation overload families

```bray
trait Reader<Mode>
{
    type Element;

    mut func read_next() -> Element?;
}

impl BufferBytesReader = Buffer(Reader<Bytes>)
{
    type Element = u8;

    mut func read_next() -> Element?
    {
        return none;
    }
}

impl BufferLinesReader = Buffer(Reader<Lines>)
{
    type Element = Line;

    mut func read_next() -> Element?
    {
        return none;
    }
}

overload Buffer(Reader) =
{
    BufferBytesReader,
    BufferLinesReader,
}

func read_byte(pos mut buffer: Buffer) -> u8?
{
    return buffer(Reader<Bytes>).read_next();
}
```

Trait satisfaction requires an explicit participating implementation for the exact `(ImplementingSubject, TraitApplication)` key. Multiple applications of one generic trait for a subject require named implementations and an explicit implementation overload family. Overlapping keys are rejected, and Bray never ranks one implementation as more specific.

## Operator contracts

```bray
impl Vec2Add = Vec2(Add<Vec2>)
{
    type Output = Vec2;

    func add(pos rhs: &Vec2) -> Output
    {
        return Vec2
        {
            x = self.x + rhs.x,
            y = self.y + rhs.y,
        };
    }
}
```

Overloadable tokens use a closed compiler-known trait set whose shared-receiver operations use non-consuming shared access.

**Remember:** Named types preserve identity, generic arguments are explicit and invariant, structural forms carry their own ownership and storage rules, traits require explicit implementations, views require an indirection boundary, and operators exist only through their exact compiler-known contracts.

## Choose the type form by intent

| Intent                                | Bray type surface                                                                                                                                          | Decisive rule                                                                                                                |
|---------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------|
| Store named fields                    | [`struct`](https://github.com/lejmer/bray/blob/develop/docs/language/types/product-types.md)                                                               | Fields form one primary representation and are immutable after initialization unless declared `mut`.                         |
| Represent closed alternatives         | [`union`](https://github.com/lejmer/bray/blob/develop/docs/language/types/union-types.md)                                                                  | Exactly one declared variant is active, and only its payload is initialized.                                                 |
| Parameterize type identity            | [Type and `const` generics](https://github.com/lejmer/bray/blob/develop/docs/language/types/generic-types.md)                                              | Arguments are explicit, ordered, invariant, and checked against declared `with(...)` constraints.                            |
| Borrow storage without ownership      | [`&T` or `&mut T`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#borrow-type-forms)                                        | Each borrow layer carries its own lifetime, capability, aliasing, and mutation contract.                                     |
| Represent optional presence           | [`T?`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#nullable-type-form)                                                   | Present stores `T`, absent is a valid initialized state, and `none` needs an expected nullable type.                         |
| Own separately stored data            | [`box T` or `box[S] T`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#owned-indirection-type-form)                         | The storage policy owns allocation and projection behavior, and `box` is the recursive ownership boundary.                   |
| Hide a concrete implementation        | [`view TraitApplication`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#trait-view-type-form) behind `&`, `&mut`, or `box` | A view is unsized and exposes only one exact trait surface through its witness.                                              |
| Store a fixed homogeneous sequence    | [`[T; N]`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#fixed-size-array-type-form)                                       | Positive compile-time length is part of the type and each element retains independent initialization state.                  |
| Access a runtime-length sequence      | [`[T]`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#slice-type-form) behind `&`, `&mut`, or `box`                        | A slice is unsized, contiguous, and never resizable by itself.                                                               |
| Combine ordered heterogeneous values  | [`(T1, T2)`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#tuple-type-form)                                                | Arity and element order define identity, while `(T,)` is distinct from grouped `(T)`.                                        |
| Store callable behavior               | [`func(...) -> R`](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-forms.md#callable-type-form)                                       | Names, `pos`, modes, ABI, effects, requirements, and result behavior all belong to callable identity.                        |
| Define reusable behavior              | [`trait` plus `impl`](https://github.com/lejmer/bray/blob/develop/docs/language/types/traits.md)                                                           | Matching member names do not imply satisfaction. One participating exact implementation is required.                         |
| Request implicit value copying        | [`@copy`](https://github.com/lejmer/bray/blob/develop/docs/language/types/copy-contracts.md)                                                               | Copying is compiler-derived, infallible, and available only when the concrete representation has no conflicting obligations. |
| Attach whole-value lifecycle behavior | [`construct`, `finalize`, `destruct`, `enter`, or `exit`](https://github.com/lejmer/bray/blob/develop/docs/language/types/lifecycle-declarations.md)       | Lifecycle declarations belong to the named type surface and interact with every represented value obligation.                |

## Common mistakes

- Do not treat two named types with matching fields or layout as interchangeable. [Named identity](https://github.com/lejmer/bray/blob/develop/docs/language/types/type-identity.md) survives representational similarity.
- Supply generic arguments in declaration order and treat generic type identity as invariant.
- Do not write redundant `public` on types, fields, traits, or inherent members unless deliberate emphasis improves the API. Public visibility is the default.
- Do not store a bare trait name. Use a generic constraint when the concrete type remains known, or place an exact `view` behind a borrow or box for dynamic dispatch.
- Use trait type-valued members and inherent type-valued bindings only for their defined associated type surfaces.
- Do not assume a `with(...)` clause introduces type parameters or activates implementations. Parameters come from declarations or implementation subjects, and implementations participate through visibility and coherence rules.
- Do not expect expected types, result types, ownership availability, or apparently narrower constraints to choose an implementation, conversion, operator, or overload arm.
- Do not confuse a mutable local binding with a mutable field. Post-initialization field mutation requires both access authority and a field declared `mut`.
- Use `@copy` for compiler-derived copying. Use a named duplication operation when behavior can fail, allocate, preserve identity, or duplicate resources.
- Do not place recursive owned storage inline through a nullable, tuple, array, payload, or generic wrapper. Every recursive ownership cycle must cross `box`.
