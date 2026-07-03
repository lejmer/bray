# Type Forms

A **type form** is a syntactic and semantic form that produces a type.

Type forms are compiler-recognized type-level constructs.

A type form defines how a type is built from one or more subject types, compile-time arguments, or structural components.

A type form can affect ownership, storage, borrowing, layout, lifetime behavior, callable behavior, initialization, destruction, finalization, access-path behavior, or value representation.

Type forms are part of the core type grammar.

Examples:

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
T?
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

## Type expressions

A **type expression** is syntax that denotes a type in a type context.

Type expressions do not read runtime storage, create runtime values, or perform runtime evaluation.

Type expressions are composed from:

- type names and qualified type paths,
- generic type applications,
- type-valued member references,
- prefix type forms,
- postfix type forms,
- structural type forms,
- type-form arguments,
- parenthesized type expressions.

Type-form arguments can include types, constants, and other compile-time entities when the type form permits them.

Trait applications can appear inside type expressions only where a type form or qualified type-valued member reference permits them.

Examples:

```bray
Point
geometry.shapes.Circle
List<Point>
Buffer(Reader<Bytes>).Element
&mut Buffer
box[Heap] Node
Point?
[Point]
[Point; 4]
(Point, Point)
func(buffer: &Buffer, index: usize) -> u8
```

A parenthesized type expression groups a single type expression.

```bray
(box[Heap] Point)?
box[Heap] (Point?)
&mut (box[Heap] Node)
```

Grouping parentheses make type-form composition order explicit.

Grouping parentheses do not produce a new type.

A parenthesized single type expression without a comma is grouping.

```bray
(Point)
```

This denotes `Point`.

Tuple type forms use comma-separated element types.

A one-element tuple type uses a trailing comma.

```bray
(Point,)
```

The trailing comma distinguishes a one-element tuple type from grouping parentheses.

## Subject type

A **subject type** is the type that a type form is applied to.

In:

```bray
box[Heap] Point
```

`Point` is the subject type.

In:

```bray
Point?
```

`Point` is the subject type.

In:

```bray
&mut Buffer
```

`Buffer` is the subject type.

The `view` type form has a trait application subject rather than a subject type.

```bray
view Sink
```

`Sink` is the trait application subject.

Some type forms have one subject type.

Some structural type forms contain multiple subject types.

```bray
(T1, T2)
func(left: T1, right: T2) -> R
```

A type form defines how its subject type or subject types contribute to the produced type.

## Type-form arguments

A type form can accept compile-time arguments.

```bray
box[Heap] T
box[AllocatorStorage<MyAllocator>] T
[T; N]
```

Type-form arguments are part of the produced type.

For example:

```bray
box[Heap] Point
box[ArenaStorage] Point
```

are distinct types because the storage policy argument differs.

A type-form argument is checked in compile-time type-form argument context.

Type-form arguments can include types, constants, and other compile-time entities when the type form permits them.

Each type form defines its permitted argument kinds.

## Prefix type forms

A **prefix type form** appears before its subject type or subject entity.

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
```

`&T` is the shared-borrow type form.

`&mut T` is the mutable-borrow type form.

`view TraitApplication` is the trait-view type form.

The `view` type form uses a trait application as its subject entity.

`box T` is the default-storage owned-indirection type form.

`box[S] T` is the owned-indirection type form using storage policy type `S`.

A prefix type form can include compile-time arguments in square brackets.

```bray
box[Heap] List<i32>
box[AllocatorStorage<MyAllocator>] Node
```

Prefix type forms compose with other type forms according to the type grammar.

```bray
&mut box[Heap] Node
box[Heap] Point?
```

Each prefix type form contributes its own ownership, storage, borrowing, lifetime, layout, initialization, destruction, finalization, and capability semantics.

## Postfix type forms

A **postfix type form** appears after its subject type.

```bray
T?
```

`T?` is the nullable type form.

It produces a type whose values and access paths have a nullable storage state.

The absence expression is `none`.

Postfix type forms compose with other type forms according to the type grammar.

```bray
Point?
box[Heap] Point?
```

Each postfix type form contributes its own value-state, ownership, initialization, destruction, finalization, and pattern behavior.

## Structural type forms

A **structural type form** uses a larger syntactic structure to produce a type.

```bray
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

`[T]` is the unsized slice type form.

`[T; N]` is the fixed-size array type form.

`(T1, T2)` is the tuple type form.

`func(...) -> R` is the callable type form.

Structural type forms can contain one or more subject types and compile-time values.

The structure of the type form is part of the type identity.

## Type-form composition

Type forms compose recursively.

```bray
box[Heap] List<i32>
box[Heap] Point?
&mut box[Heap] Node
box[Heap] view Sink
box[Heap] [u8]
[box[Heap] Node; 4]
func(buffer: &Buffer, index: usize) -> u8
```

The meaning of a composed type is determined by applying each type form according to the type grammar and the semantic contract of that form.

Composition order is determined by the type grammar and explicit grouping parentheses.

A composed type has one ownership, borrowing, initialization, destruction, finalization, capability, effect, and layout contract produced by the composition of its type forms and subject types.

If two composed type forms impose incompatible requirements, the composed type is rejected.

## Borrow type forms

The shared-borrow type form is:

```bray
&T
```

The mutable-borrow type form is:

```bray
&mut T
```

A borrow type is a non-owning access path type.

A borrow value does not own the reached storage.

A borrow has a lifetime.

A borrow is valid only while the reached storage remains valid and the borrow’s capability contract remains satisfied.

A shared borrow allows observation.

A mutable borrow grants temporary exclusive mutation authority over the reached storage.

Multiple compatible shared borrows can exist at the same time.

While a mutable borrow is active, incompatible operations on the borrowed storage are suspended.

Borrowing suspends movement, destruction, mutation, variant replacement, reinitialization, finalization, or other incompatible
operations on the reached storage for the duration of the borrow.

Borrow compatibility is based on the reached storage, not only on the spelling of the access path.

Two borrow access paths are compatible when their required capabilities are compatible and the compiler proves that the reached
storage is the same shared-readable storage or statically disjoint storage.

Shared borrows of the same reached storage are compatible.

A mutable borrow of reached storage is incompatible with any other active borrow or operation that observes, mutates, moves,
destroys, finalizes, reinitializes, or changes that reached storage, except through a valid reborrow derived from the mutable
borrow.

Disjoint field projections, tuple element projections, active payload projections, indexed element projections, and slice ranges
can be borrowed independently when the compiler proves that the reached storage cannot overlap.

If overlap cannot be proven statically, the borrow access paths are treated as conflicting.

Borrow values are created only by language constructs that establish borrow capability.

These include borrow expressions, borrow-typed parameter passing, receiver calls, pattern borrow modes, and projection through
compiler-known type forms.

Borrowing an access path requires the reached storage to be initialized and reachable.

A shared borrow requires an access path that can be observed.

A mutable borrow requires mutation authority over the reached storage and compatible exclusivity for the duration of the borrow.

A shared borrow value is copyable.

Copying a shared borrow copies the borrow value and preserves the same reached storage, lifetime, and capability requirements.

A mutable borrow value is not copyable.

Moving a borrow value moves only the borrow value.

Moving a borrow value never moves the reached storage.

Moving a mutable borrow transfers its exclusive mutation authority to the destination borrow value.

A borrow lifetime can be shortened to the last required use of the borrow.

A reborrow can be created from an existing borrow.

A reborrow has authority no greater than the borrow it comes from.

A reborrow suspends incompatible use of the original borrow for the reached storage while the reborrow is active.

A borrow value can be stored only when the containing value's type and contract carry the borrow's lifetime and capability
requirements.

Storing a borrow does not extend the lifetime of the reached storage.

A callable can return a borrow only when the callable result contract preserves the lifetime and capability dependency on a
parameter, receiver, or other input storage that can outlive the returned borrow.

A callable cannot return a borrow of local storage that ends before the returned borrow.

A borrow becomes invalid when the reached storage stops being valid, when the borrow's capability contract stops being satisfied, or
when the nullable access path holding the borrow is assigned `none`.

Mutation through a valid mutable borrow invalidates facts that depend on the changed storage.

Movement, destruction, reinitialization, finalization, active-variant replacement, or capability loss invalidates facts and borrows
that depend on the affected storage.

Nested borrow types are allowed.

```bray
&&T
&&mut T
&mut &T
&mut &mut T
```

Each borrow layer has its own capability.

An outer shared borrow provides shared access to the next layer.

An outer mutable borrow provides mutation authority over the next layer.

The reachable operation depends on the whole access path, including every borrow layer.

## Lifetime and capability dependency contracts

Bray does not have source-level lifetime parameters or source-level lifetime annotations.

Lifetime and capability dependency contracts are semantic facts inferred and checked by the compiler.

A **dependency contract** records the non-local requirements that must remain true for a value, access path, callable value, trait
view, task handle, thread handle, or stored field to remain valid.

A dependency contract can include:

- storage that must remain alive,
- storage that must remain initialized,
- borrow capability that must remain active,
- mutation authority that must remain exclusive,
- scoped capability that must remain live,
- finalization or destruction obligations that must remain attached to the value,
- facts whose validity depends on the same storage, capability, or ownership state.

A dependency contract is not part of surface syntax.

It is part of the compiler-visible semantic contract of the value or declaration that carries it.

For exported declarations, compiled interfaces, documentation, diagnostics, incremental compilation, and separate compilation, the
compiler records the inferred dependency contract as interface metadata.

Two compilers must reject and accept the same programs according to the language rules for dependency contracts, even though the
source code does not write those contracts explicitly.

Each expression that produces a value or access path also produces a dependency contract.

Owned values carry the dependency contracts of their initialized subvalues.

Borrow values carry the reached storage, borrow capability, and invalidation requirements of the borrow.

Nullable values carry the dependency contract of their contained value only while present.

Product, union, tuple, array, and box values carry the dependency contracts of the parts they currently own or borrow.

Callable values carry the dependency contract of the callable declaration or lambda expression that produced them.

Lambda expressions do not capture enclosing local state.

A trait view carries the dependency contract of the access or storage form that contains the view, plus the requirements of the
implementation witness needed for the selected trait application.

A task handle carries the dependency contract of the captured task state and the task obligation represented by the handle.

A thread handle carries the dependency contract of the captured thread state and the thread obligation represented by the handle.

Moving a value moves its dependency contract with the value.

Copying a value copies its dependency contract only when the value's copy contract permits the copy.

Destroying, finalizing, cancelling, joining, assigning `none`, or otherwise resolving a value resolves or invalidates the dependency
contract carried by that value according to the operation's contract.

A value can cross a boundary only when the destination can preserve every dependency contract carried by that value.

Boundaries include:

- returning from a callable,
- yielding from a value-producing region,
- storing into a field, variant payload, tuple element, array element, or box storage,
- assigning to an existing access path,
- passing an argument to a callable,
- capturing into an async computation, task, or thread,
- forming a trait view,
- importing or exporting a declaration surface.

A destination preserves a dependency contract when the destination's lifetime, ownership state, capability state, and semantic
contract are proven to keep every required storage, borrow, capability, and obligation valid until the destination no longer uses or
owns the value.

Storing a dependency-carrying value never extends the source storage or scoped capability that the dependency requires.

If the destination could outlive required storage or a required scoped capability, the transfer is rejected.

If a dependency contract cannot be represented in the destination's compiler-visible semantic contract, the transfer is rejected.

Function, method, constructor, lifecycle, lambda, async, and implementation bodies are checked against their inferred dependency
contracts.

At each normal exit, the result value's dependency contract must be derived from parameters, receiver state, owned input values,
async or task state available to that body, or other storage and capabilities that outlive the returned value.

Returning a borrow of local storage that ends at the callable exit is rejected.

Returning a value that owns local state is valid when ownership moves into the result and the value's dependency contract no longer
depends on the local binding.

When multiple control-flow exits can produce a value, the merged dependency contract conservatively preserves every dependency that
can be required by any reachable exit unless facts available at the use site prove a narrower alternative.

The expected result type, assignment target type, or overload result type does not invent missing dependency contracts.

Dependency contracts are inferred from the producing expression and checked against the destination.

A declaration whose public result, stored value, callable value, trait view, task handle, thread handle, or lifecycle value carries
non-local dependencies exposes those dependencies through its compiler-visible declaration contract.

This exposure is semantic metadata, not additional source syntax.

Callers must satisfy the exposed dependency contract when they use the declaration.

This is valid because the returned borrow depends on the `item` parameter:

```bray
func same(pos item: &u8) -> &u8
{
    return item;
}
```

The returned borrow remains valid only while the storage reached through `item` remains valid and while the returned borrow's
capability requirements remain satisfied.

This is rejected because the returned borrow depends on local storage that ends at the function exit:

```bray
func bad() -> &u8
{
    let value: u8 = 1;
    return &value;
}
```

This type carries the dependency contract of its `data` field:

```bray
struct Cursor
{
    data: &[u8];
    index: usize;
}
```

A `Cursor` value cannot outlive the storage reached by `data`.

A lambda cannot capture a borrow from an enclosing local binding:

```bray
let writer =
{
    let output = &mut file;

    lambda (pos text: string)
    {
        output.write(text); // invalid
    }
};
```

Pass the state explicitly when a callable value needs caller-provided context:

```bray
let write_line = lambda (pos output: &mut File, pos text: string)
{
    output.write(text);
};
```

## Nullable type form

The nullable type form is:

```bray
T?
```

A nullable value or access path has a nullable storage state.

The nullable storage state is either present or absent.

When present, the nullable value contains or reaches a value of type `T`.

When absent, the nullable value contains or reaches no value or storage of type `T`.

The absent state is a valid initialized state.

A nullable value or access path is fully initialized when it is initialized to either present or absent state.

When present, the contained `T` value follows the ownership, borrowing, movement, copying, destruction, finalization, and capability rules of `T`.

When absent, there is no contained `T` value to access, move, copy, borrow, destroy, or finalize.

`T?` is not a named container type.

It does not expose a user-visible variant container or construction wrapper.

The absence expression is:

```bray
none
```

`none` has no standalone type.

`none` is accepted only when the expected type determines a concrete nullable type `T?`.

```bray
let mut count: i32? = none;
count = 10;
count = none;
```

A value of type `T` can initialize or assign the present state of an expected `T?`.

Assigning `none` to a nullable access path changes its nullable storage state to absent.

If the access path owns a present value, that value is destroyed before the access path becomes absent.

If the access path holds a borrow, assigning `none` ends the borrow before the access path becomes absent.

The binding, field, parameter, or other declaration remains declared; only the nullable storage state changes.

Nullable-to-nullable conversion follows the recursive explicit convertibility rule when the contained source type is explicitly convertible to the contained target type.

The absent state remains absent during nullable-to-nullable conversion.

A present value is converted recursively.

Nullable pattern rules are defined in [Nullable patterns](../patterns/nullable-patterns.md).

Nullable propagation expression rules are defined in [Nullable and absence expressions](../expressions/nullable-and-absence-expressions.md).

## Owned-indirection type form

The owned-indirection type form is:

```bray
box T
box[S] T
```

`box T` uses the default storage policy.

`box[S] T` uses storage policy type `S`.

`T` is the contained subject type.

`S` is the storage policy type.

For sized `T`, the storage policy type must satisfy `Storage<T>`.

The storage policy is an ordinary type that satisfies the compiler-known `Storage<T>` trait for the stored type.

The `Storage<T>` trait is declared by the language substrate and interpreted by the `box` type form.

Implementing `Storage<T>` is ordinary trait implementation plus the trusted declarations required by the storage operations.

The compiler does not infer storage behavior from matching member names.

The compiler recognizes the trait identity of `Storage<T>` and the contracts of its required members.

The core storage contract is:

```bray
trait Storage<T>
{
    trusted static func create(pos value: T) -> Self
        uses(manual_alloc, raw_memory, unchecked_init);

    static func borrow(pos storage: &Self) -> &T;

    static func borrow_mut(pos storage: &mut Self) -> &mut T;

    trusted static func destroy(pos storage: &mut Self)
        uses(raw_memory, unchecked_init);

    trusted static func release(pos storage: Self)
        uses(manual_alloc);
}
```

`Storage<T>.create` creates storage for a fully initialized `T` and moves `value` into that storage.

`Storage<T>.borrow` projects a shared borrow of the stored `T`.

`Storage<T>.borrow_mut` projects a mutable borrow of the stored `T`.

`Storage<T>.destroy` destroys the stored `T` without releasing the storage object itself.

`Storage<T>.release` releases the storage object after the stored value has been destroyed or otherwise removed according to the storage contract.

`Storage<T>.create` requires `T` to be sized.

The `box[S] T` type form can store an unsized subject only when the type-form rule defines how to store a sized concrete value behind that subject.

For `box[S] view TraitApplication`, box construction stores a sized concrete value `U` using `Storage<U>`, then forms the view from the stored `U` and the selected `U(TraitApplication)` implementation witness.

For `box[S] [T]`, box construction uses contiguous owned storage behavior for element type `T` and a runtime element count.

The storage policy allocates storage for the element count, initializes each element exactly once, projects slice borrows, destroys
initialized elements, and releases the allocation according to the storage policy.

```bray
let sink: box[Heap] view Sink = box[Heap](file_sink);
```

Here `file_sink` has a sized concrete type such as `FileSink`.

The storage policy must satisfy `Storage<FileSink>`.

The view type is `view Sink`.

The resulting box owns the stored `FileSink` and carries the implementation witness for `FileSink(Sink)`.

The default storage policy is `Heap`.

A storage policy can require construction arguments.

Those arguments are supplied to the box construction expression after the contained value argument.

```bray
let point: box[AllocatorStorage<MyAllocator>] Point =
    box[AllocatorStorage<MyAllocator>](
        { x = 1.0, y = 2.0, },
        storage = storage,
    );
```

Storage construction arguments are part of the storage policy's construction contract.

They are not part of the `Storage<T>` type application.

A `Storage<T>` implementation must preserve Bray ownership, borrowing, initialization, destruction, finalization, capability, effect, and trusted-obligation rules.

Trusted storage members expose implementation power only inside their bodies.

Calling `box(...)`, borrowing through a box, and destroying a box remain ordinary operations when the selected storage implementation satisfies its public contract.

For sized `T`, a `box[S] T` value owns separately stored `T`.

For `box[S] view TraitApplication`, the box owns the stored concrete value and exposes it through the trait-view subject.

For `box[S] [T]`, the box owns the contiguous element storage and exposes it through the slice subject.

Moving a `box[S] T` moves ownership of the indirection value.

Destroying a `box[S] T` destroys the stored value and releases storage according to the storage policy.

Borrowing a `box[S] T` can project a borrow of the contained or viewed value when the storage policy and access path permit it.

Mutable borrowing a `box[S] T` can project mutable access to the contained or viewed value when the box access path, storage policy, and contained type permit it.

The outer representation of `box[S] T` has statically known finite size independent of `T`.

`box[S] T` can serve as an indirection boundary for recursive types.

Box construction is handled by box construction expressions.

```bray
let node: box List<i32> = box(.Empty);
```

Box construction expression rules are defined in [Box construction expressions](../expressions/box-construction-expressions.md).

## Trait-view type form

The trait-view type form is:

```bray
view TraitApplication
```

`TraitApplication` must be an exact trait application.

If the trait declaration is generic, the view type must supply the generic arguments required by the trait application.

```bray
view Sink
view Encoder<Json>
```

A trait view exposes a hidden concrete value through the callable and contract surface of one trait application.

The concrete implementing type is not part of the visible static type.

A trait view carries the implementation witness needed to dispatch calls through the selected trait implementation.

The implementation witness is part of the trait view's dependency contract.

The runtime representation of a trait view is compiler-defined.

It must preserve the view's ownership, borrowing, lifetime, destruction, finalization, capability, effect, and contract semantics.

`view TraitApplication` is unsized.

It is not a storable value type by itself.

It must appear behind a type form that defines storage or access for an unsized subject.

The defined forms are:

```bray
&view TraitApplication
&mut view TraitApplication
box[S] view TraitApplication
```

This is rejected:

```bray
let sink: view Sink = file_sink;

struct Logger
{
    sink: view Sink;
}
```

This is valid:

```bray
struct Logger
{
    sinks: [box[Heap] view Sink; 4];
}
```

A trait view is not a dynamic type.

It does not permit runtime type tests, downcasting, field access on the hidden concrete type, or calls outside the selected trait view surface.

The only behavior available through a view is behavior declared by the exact trait application and accepted by the view-surface rules.

A concrete value can form a view only when its type satisfies the exact trait application through a participating implementation.

If no participating implementation satisfies the exact trait application, view formation is rejected.

If more than one participating implementation could satisfy the exact trait application, view formation is rejected by coherence rules before the view is formed.

Static functions in a trait are not part of a value view surface.

A callable trait member is part of a view surface only when its signature, contracts, effects, capabilities, and obligations can be checked without naming the hidden concrete type.

A callable trait member that mentions `Self` outside the receiver is not part of a view surface.

A callable trait member with its own generic parameters is not part of a view surface.

A callable trait member that exposes an unfixed type-valued member is not part of a view surface.

A trait with type-valued members can still be used statically through generic constraints and exact trait applications.

When runtime dispatch must expose a related type through a view, that related type must be represented as an input to the trait application rather than as a type-valued member output.

For example:

```bray
trait Stream<Item>
{
    mut func next() -> Item?;
}

let stream: &mut view Stream<Token> = &mut token_stream;
```

A shared borrowed view permits shared receiver methods.

A mutable borrowed view permits shared and mutable receiver methods.

An owned boxed view permits shared, mutable, and consuming receiver methods according to the box access path, ownership state, and receiver mode.

Consuming a boxed view consumes the owning box value.

The hidden concrete value is destroyed and finalized according to the selected implementation, the concrete type, and the storage policy.

## Fixed-size array type form

The fixed-size array type form is:

```bray
[T; N]
```

`T` is the element type.

`N` is the array length.

`N` is part of the type.

`N` must be greater than zero.

A fixed-size array contains exactly `N` elements of type `T`.

Each element has its own initialization state while the array is being initialized or after a partial move.

An array value is fully initialized when every element is initialized.

Array element access uses indexing syntax.

An array index must be a nonnegative integer index accepted by the array indexing contract.

For an array of length `N`, an element index is valid only when it is less than `N`.

Indexing a fixed-size array access path reaches the selected element access path.

Slicing a fixed-size array access path reaches contiguous substorage with slice type `[T]`.

Array construction is handled by array expressions and array generator expressions.

Moving a complete array moves every initialized element as part of the array value.

Copying an array requires the element type to satisfy the required copy contract.

Borrowing an array borrows the array storage. Element access and slice projection can derive narrower borrows from that borrow
when ordinary borrowing rules permit it.

Moving an element out of an owned array access path is a partial move of the array.

After an element has been moved out, the array is partially initialized.

Destruction of a partially initialized array destroys only initialized elements.

Array destruction processes initialized elements in reverse index order.

Finalization obligations retained by array elements are retained by the array.

Array expression rules are defined in [Array expressions](../expressions/array-expressions.md) and [Array generator expressions](../expressions/array-generator-expressions.md).

Array indexing and slicing expression rules are defined in [Index access expressions](../expressions/index-access-expressions.md).

## Slice type form

The slice type form is:

```bray
[T]
```

`T` is the element type.

`[T]` is an unsized contiguous sequence type.

The slice length is runtime state carried by an indirection boundary.

A slice type cannot appear as a local value by itself, a by-value parameter type, a by-value result type, or a by-value field type.

A slice type can appear behind an indirection boundary:

```bray
&[T]
&mut [T]
box[S] [T]
```

`&[T]` is a shared slice borrow.

`&mut [T]` is a mutable slice borrow.

`box[S] [T]` is owned contiguous slice storage using storage policy `S`.

Owned slice storage has a fixed length after construction.

Resizable buffers are library types built on top of storage primitives rather than a meaning of `[T]`.

A shared slice borrow permits observation of initialized elements.

A mutable slice borrow permits mutation of initialized elements according to ordinary exclusive-borrow rules.

Borrowed slices do not own their elements.

Moving an element out through a borrowed slice is not allowed.

Moving `box[S] [T]` moves the owned contiguous storage, its runtime length, and its initialized elements as one owned value.

Destroying `box[S] [T]` destroys initialized elements in reverse index order and then releases the underlying storage through `S`.

Borrowing `box[S] [T]` can produce `&[T]` or `&mut [T]` according to the borrowing mode and access path authority.

Indexing `box[S] [T]` projects through the owned indirection to the selected element.

Slicing `box[S] [T]` projects through the owned indirection to contiguous substorage.

Slice projection never copies elements.

Slice projection cannot change the length of the projected storage.

## Tuple type form

A tuple type form is:

```bray
(T1, T2)
```

A tuple type is a fixed-size ordered product type.

The arity of a tuple is the number of element types.

The element types are part of the tuple type in order.

A one-element tuple type uses a trailing comma.

```bray
(T,)
```

The trailing comma distinguishes a one-element tuple type from grouping parentheses.

Each tuple element has its own type and initialization state.

A tuple value is fully initialized when every element is initialized.

Tuple construction is handled by tuple expressions.

Moving a complete tuple moves every initialized element as part of the tuple value.

Copying a tuple requires every element type to satisfy the required copy contract.

Borrowing a tuple borrows the tuple storage. Tuple element projection can derive narrower borrows from that borrow when ordinary
borrowing rules permit it.

Moving an element out of an owned tuple access path is a partial move of the tuple.

After an element has been moved out, the tuple is partially initialized.

Destruction of a partially initialized tuple destroys only initialized elements.

Tuple destruction processes initialized elements in reverse element order.

Finalization obligations retained by tuple elements are retained by the tuple.

Tuple expression rules are defined in [Tuple expressions](../expressions/tuple-expressions.md).

Tuple element projection uses dot-number syntax.

```bray
pair.0
pair.1
```

The projection number is a compile-time tuple element position.

The selected position must be within the tuple arity.

Tuple element projection reaches the selected element access path.

Tuples do not support bracket indexing or slicing.

## Callable type form

The callable type form is:

```bray
func(parameter: Type, ...) -> Result
```

A callable type describes a callable value’s parameter names, parameter call-position permissions, parameter types, result type, execution mode, callable ABI, ownership behavior, borrowing behavior, mutation requirements, lifetime requirements, capability requirements, caller-visible effects, trusted caller obligations, and finalization behavior.

Callable parameter names and `pos` permissions are part of the callable contract because they define how call arguments bind to parameters.

```bray
func(left: i32, right: i32) -> i32
func(pos value: i32) -> i32
const func(pos value: i32) -> i32
async func(pos request: Request) -> Response
@abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32
```

A callable returning `unit` can omit the result type.

Callable types preserve caller-visible obligations.

Const eligibility is a caller-visible callable contract.

A callable with trusted caller obligations requires a callable type that preserves those obligations.

Callable values and callable declarations are checked by [Callables](../callables.md).

Contract clauses attach after the callable type signature.

```bray
func(pos value: i32) -> i32
    requires(value >= 0)
```

Contract clauses on callable type forms use the same predicate-expression syntax as contract clauses on callable declarations.

Const callable type forms use `const` before `func`.

```bray
const func(pos value: i32) -> i32
```

Async callable type forms use `async` before `func`.

```bray
async func(pos request: Request) -> Response
```

Trusted callable type forms preserve trusted callable obligations and trusted implementation capability requirements through their
callable contract.

```bray
trusted func(pos bytes: &mut [u8])
    uses(raw_memory)
```

ABI-qualified callable type forms use `@abi(...)` immediately before `func`.

```bray
@abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32
```

Callable ABI is part of the callable type's visible contract.

A named callable contract declaration gives a reusable name to a callable type form. Named callable contract rules are defined in [Callable types and values](../callables/callable-types-and-values.md).

```bray
callable Transform =
    func(pos value: i32) -> i32;
```

The callable name can be used in type positions that expect a callable type.

A named callable contract can be generic.

```bray
callable Mapper<T, U> =
    func(pos value: T) -> U;
```

A named callable contract can have `with(...)` constraints.

```bray
callable OrderedPredicate<T>
    with(T: Comparable<T>) =
    func(pos left: &T, pos right: &T) -> bool;
```

A named callable contract is not a general type alias.

It can name only callable type forms.

It does not rename arbitrary type expressions, functions, methods, lambdas, implementations, modules, or packages.

A callable value satisfies a named callable contract when its visible callable contract satisfies the named contract.

Named callable contracts cannot be overloaded.

Lambda expressions produce anonymous callable values. Lambda callable rules are defined in [Lambda expressions and anonymous callables](../callables/lambda-expressions-and-anonymous-callables.md).

A lambda's callable type is described by its parameter names, parameter call-position permissions, parameter types, result type,
execution mode, callable ABI, contract clauses, and trusted obligations.

Lambdas are capture-free.

A lambda can be used where an expected callable type accepts a callable value with the same visible callable contract.

The call surface of a lambda is its visible callable contract.

## Type forms and construction

Some type forms define construction expression syntax.

`box` defines a type-form construction expression.

```bray
box[S] T
box[S](value, ...)
```

A type form with construction behavior defines how a value of the produced type is constructed from runtime expressions and compile-time arguments.

A type-form construction expression is compiler-recognized.

A type-form construction expression can invoke ordinary declarations, trait behavior, lifecycle declarations, storage behavior, trusted declarations, and contract clauses according to the type form’s rules.

A type form with no construction behavior cannot be used as a construction expression through type-form syntax.

Type-form construction expression rules are defined in [Type-form construction expressions](../expressions/type-form-construction-expressions.md).

## Type forms and recursive types

A stored field type must have known finite outer size.

A recursively reachable stored field is valid only when every recursive path back to the declaring type crosses an owning indirection boundary.

An **owning indirection boundary** is a type form whose outer value has known finite size and owns storage for its subject separately from the outer value.

The owning indirection boundary is `box[S] T`.

This includes `box[S] view TraitApplication`.

```bray
struct Node
{
    next: Self; // invalid
}

struct Node
{
    next: box[Heap] Self; // valid
}
```

Borrow types are finite access values.

Borrow types are non-owning and do not make a type own recursive structure.

```bray
struct View
{
    next: &Self; // valid finite representation, non-owning
}
```

Structural type forms do not break recursive stored ownership.

Products, union payloads, tuples, arrays, and nullable values keep their stored subjects inline for recursive sizing.

```bray
struct Node
{
    next: Self?; // invalid
}

struct Node
{
    children: [Self; 2]; // invalid
}

struct Node
{
    next: (box[Heap] Self)?; // valid
}
```

Generic type applications do not hide recursive storage.

After generic substitution, the resulting stored field type must satisfy the same recursive stored-field rules.

```bray
struct Wrap<T>
{
    value: T;
}

struct Node
{
    next: Wrap<Self>; // invalid
}
```

## Type forms and layout

A type form contributes layout constraints to the produced type.

Some type forms have compiler-defined default layout.

Some type forms can use representation optimizations when their semantics are preserved.

Stable ABI layout, C-compatible layout, explicit representation, explicit alignment, and packed layout belong to explicit layout
contracts.

Layout directives use `@layout(...)`.

```bray
@layout(mode, option = value, ...)
```

The first argument is the layout mode.

Remaining arguments are named layout options.

Only one `@layout(...)` directive can apply to a primary representation declaration.

`@layout(...)` attaches to primary representation declarations, not to arbitrary type expressions or implementation blocks.

For user-declared types, the primary representation declarations that accept `@layout(...)` are `struct` and `union`.

Default layout is compiler-defined.

Without `@layout(...)`, a product type's field declaration order and a union type's variant declaration order remain semantic,
but physical field offsets, physical tag representation, padding, size, alignment, and representation optimizations are selected
by the compiler.

Default layout is not public ABI.

The compiler can select different default layouts across targets, compiler versions, optimization profiles, and code generation
strategies when Bray semantics are preserved.

Source code can depend on physical layout only when an explicit layout contract is declared.

The Raw Memory Model's `std.memory.size_of<T>()`, `std.memory.align_of<T>()`, `std.memory.stride_of<T>()`, and
`std.memory.layout_of<T>(count = count)` helpers observe the effective layout contract produced by these rules.

For a type with an explicit `@layout(...)` directive, those helpers report the layout defined by that directive and the selected
target profile.

For a type with compiler-defined default layout, those helpers report the compiler-selected layout for the selected target profile
without making that layout public ABI.

The layout modes are:

- `stable`,
- `c`,
- `transparent`.

`stable` declares a Bray-defined physical layout for the target layout profile.

`c` declares target C ABI-compatible physical layout.

`transparent` declares that a product type has the same physical layout and ABI as its single storage field.

The layout options are:

- `align = N`,
- `pack = N`,
- `tag = IntegerType`.

`align = N` raises the aggregate alignment to at least `N`.

`N` must be a compile-time integer constant and a power of two.

`align` is valid for `stable` and `c` layout.

For `c` layout, the requested alignment must be representable by the target C ABI.

`pack = N` caps field and payload alignment at `N`.

`N` must be a compile-time integer constant and a power of two.

`pack` is valid only for `stable` layout.

`pack` is valid only when the laid-out representation is plain storage.

Plain storage means every represented field and payload is recursively plain storage and the representation has no destructor,
finalizer, scoped lifecycle declaration, ownership obligation, resource obligation, finalization obligation, borrow field, `box`,
trait-view field, callable field, task field, or protected representation.

Packed fields and packed payload components can be loaded and stored by value according to the packed layout contract.

A packed field or payload component cannot be borrowed as `&T` or `&mut T` unless the compiler proves that the specific access is
naturally aligned for `T`.

`tag = IntegerType` sets the physical tag type of a union.

`tag` is valid only for union layout.

The tag type must be an integer scalar type.

Layout directives do not change ownership, initialization, destruction, finalization, field access, variant access, method
resolution, trait satisfaction, or contract semantics.

Padding bytes are not semantic values.

Padding bytes are not guaranteed initialized unless represented by explicit fields.

Reading, writing, or reinterpreting padding or representation bytes requires a trusted operation with the appropriate trusted
capability.

Changing a public type's explicit layout contract is a public API and ABI change.

## Type-form eligibility

A type form is introduced by the language when the form has core semantic meaning.

A type form earns core status when ordinary named types and behavioral contracts cannot express the construct without losing required compiler knowledge about ownership, storage, borrowing, layout, lifetime behavior, callable behavior, initialization, destruction, or finalization.

`box` is a type form because owned indirection affects recursive type sizing, ownership transfer, destruction, borrow projection, and storage identity.

`?` is a type form because nullability is a core value-state shape used throughout the language.

`func(...) -> ...` is a type form because callable values carry parameter, result, ownership, effect, execution, and contract semantics.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Union Types](union-types.md)
- Next: [Traits](traits.md)
