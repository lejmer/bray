# Implementations

An **implementation** declares behavior for a type.

Implementations are declared with `impl`.

An inherent implementation is written as `impl Type`.

```bray
impl Point
{
    func distance_to(pos other: &Self) -> r64
    {
        ...
    }

    static func origin() -> Point
    {
        ...
    }
}
```

An inherent implementation adds behavior associated with the type.

An inherent implementation does not add fields to the type’s primary representation.

A trait implementation can be unnamed or named.

An unnamed trait implementation is written as `impl ImplementingSubject(TraitApplication)`.

```bray
impl Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A named trait implementation is written as `impl ImplementationName = ImplementingSubject(TraitApplication)`.

```bray
impl PointEquatable = Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

`ImplementationName` is the implementation identity.

It is not a type, a type alias, or a wrapper around the implementing subject.

The implementing subject before the parentheses determines `Self`, the receiver type, and the storage being implemented for.

The trait application inside the parentheses determines the contract being fulfilled.

For a generic trait application:

```bray
impl Point(Comparable<Point>)
{
    func compare(pos other: &Point) -> Ordering
    {
        ...
    }
}
```

For a generic implementing subject:

```bray
impl BufferEquatable = Buffer<T>(Equatable<Buffer<T>>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A trait implementation makes the implementing subject satisfy the specified trait application.

Trait satisfaction is explicit.

An implementing subject satisfies a trait application through an accepted participating implementation declaration for that exact subject and trait application.

Matching member names and signatures alone gives no trait satisfaction.

## Inherent implementation ownership

A source type's defining package owns its inherent implementation surface.

Only that package can declare an inherent implementation for the type. The implementation can appear in any module included in the
selected source graph for the package product.

A compiler-known type is owned by the compiler-known environment. Source and standard-library packages cannot add inherent
implementations to a compiler-known type unless an owning language rule explicitly assigns that authority.

Trait implementations remain the extension mechanism for behavior declared outside the semantic owner of a type.

Every enabled inherent implementation from the owning package's selected source graph contributes automatically to the associated
type. A `using` declaration or export does not activate or deactivate an inherent implementation.

## Trait implementation subjects

An implementation subject is the entity being implemented for.

Inherent implementations require a named type subject.

Trait implementation subjects can be:

- a named type subject,
- a generic named type subject,
- an inferred implementation parameter used as a type subject,
- an implementation-eligible type-form subject.

Implementation-eligible type-form subjects are:

- `&T`,
- `&mut T`.

No other type form is implementation-eligible unless this chapter explicitly defines it as implementation-eligible.

These are distinct exact implementation subjects:

```bray
impl VecReadIterable = &Vec<T>(Iterable)
{
    type Element = &T;
    type Cursor = VecReadCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}

impl VecWriteIterable = &mut Vec<T>(Iterable)
{
    type Element = &mut T;
    type Cursor = VecWriteCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}

impl VecMoveIterable = Vec<T>(Iterable)
{
    type Element = T;
    type Cursor = VecMoveCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}
```

`Vec<T>`, `&Vec<T>`, and `&mut Vec<T>` are three different implementing subjects.

An implementation for `&T` does not make `T` satisfy the same trait application.

An implementation for `&mut T` does not make `&T` satisfy the same trait application.

A value of type `&mut T` can reach an implementation for `&T` only when ordinary expression rules create an explicit shared reborrow and method resolution is then performed on the shared-borrow type.

For a trait implementation with an implementation-eligible type-form subject, `Self` is the whole type form.

Inside `impl VecReadIterable = &Vec<T>(Iterable)`, `Self` is `&Vec<T>`.

Inside `impl VecWriteIterable = &mut Vec<T>(Iterable)`, `Self` is `&mut Vec<T>`.

Values returned by callable members can carry borrow dependencies from a borrow implementation subject according to ordinary dependency-contract rules.

Such values cannot outlive or extend the borrow represented by the implementation subject.

## Implementation members

An implementation body contains member definitions.

In an inherent implementation, member definitions become declarations associated with the implementing type.

In a trait implementation, member definitions fulfill members of the implemented trait application.

```bray
impl Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A trait implementation must provide every required callable trait member that has no default body.

A trait implementation must provide every required constant-valued member.

A trait implementation must bind every required type-valued member.

A trait implementation must provide every required predicate member.

A trait implementation must satisfy every lifecycle requirement.

A trait implementation can provide a callable member or constant-valued member that has default behavior in the trait.

When a trait implementation provides a callable member or constant-valued member with default behavior, the implementation member
is used for that implementation.

When a trait implementation omits a callable member or constant-valued member with default behavior, the trait’s default behavior
is used for that implementation.

An inherent implementation can bind a type-valued member for its implementation subject.

```bray
impl Buffer<T>
{
    type Cursor = BufferCursor<T>;
}
```

An inherent type-valued member binding introduces a type-associated member of the implementation subject.

The selected type can depend on `Self`, inferred implementation parameters, and static constraints established by the implementation.

The selected type cannot depend on a runtime value, control-flow path, local inference choice, caller preference, or use site.

An inherent type-valued member is reached through ordinary type-associated lookup.

```bray
Buffer<u8>.Cursor
```

An inherent type-valued member binding is not a module-level type alias.

An inherent type-valued member binding participates in the type-associated aggregation and conflict rules defined below.

A trait implementation callable member must match the fulfilled trait member’s name, receiver mode, parameter names, parameter types, result type, execution mode, contract obligations, and caller-visible effects after type-valued member bindings have been applied.

A trait implementation constant-valued member must match a constant-valued member declared by the implemented trait.

A trait implementation constant-valued member must use the same declared type as the fulfilled trait member after type-valued member bindings have been applied.

A trait implementation constant-valued member initializer must be a constant expression valid in the implementation context.

A trait implementation cannot provide the same constant-valued member more than once.

A trait implementation cannot provide extra constant-valued members that are not declared by the trait.

A trait implementation type-valued member binding must match a type-valued member declared by the implemented trait.

A trait implementation type-valued member binding must select a concrete type that is valid in the implementation context.

A trait implementation cannot bind the same type-valued member more than once.

A trait implementation cannot provide extra type-valued member bindings that are not declared by the trait.

A trait implementation predicate member must match a predicate member declared by the implemented trait.

A trait implementation predicate member body must be a predicate expression valid in the implementation context.

A trait implementation cannot provide the same predicate member more than once.

A trait implementation cannot provide extra predicate members that are not declared by the trait.

A trait implementation lifecycle declaration can fulfill only an `enter` or `exit` requirement declared by the implemented trait.

A trait implementation lifecycle declaration must satisfy the lifecycle signature and contract clauses of the requirement it
fulfills after type-valued member bindings have been applied.

A trait implementation cannot provide `finalize` or `destruct` as trait implementation members.

A trait implementation cannot provide lifecycle declarations that do not fulfill lifecycle requirements declared by the trait.

A trait implementation cannot provide constructor declarations.

A trait implementation cannot provide callable overload declarations.

An inherent member's effective reachability is governed by its associated type, declaring module, and member visibility.

Individual trait implementation members cannot use `public` or `internal` modifiers.

Implementation members in a trait implementation are fulfillments of a trait contract, not independent visibility surfaces.

## Type-associated member aggregation

A named type has one type-associated surface for each selected package product and target.

That surface contains:

- representation and behavior members declared directly in the type body,
- members from every enabled inherent implementation owned by the type,
- callable overload families declared in either location,
- the type's lifecycle declarations,
- the inherent implementation declarations that contributed those members.

Trait implementation members are not part of this surface. They remain fulfillments of an exact trait application and participate
through trait implementation and method-resolution rules.

Union payload fields remain in the scope of their variant payload. They do not become union-wide associated members.

### Member identity and ownership

Aggregation does not create replacement declarations.

A member declared directly in a type body remains declared by that type. A member declared in an inherent implementation remains
declared by that implementation. Both are associated with the same type definition and participate in lookup through that type.

Declaration identity, source location, effective visibility, implementation constraints, and declaring implementation remain
observable for diagnostics, documentation, navigation, and public API compatibility.

### Associated name conflicts

Direct type members and inherent implementation members share the type's ordinary lookup namespace.

The same ordinary name cannot be introduced by:

- a direct member and an inherent member,
- members from different inherent implementations,
- members of different semantic categories,
- members with different visibility,
- members guarded by different generic constraints.

A direct declaration does not take precedence over an inherent declaration. Source order, module order, visibility, and apparently
more-specific constraints do not choose a winner.

Different constrained inherent implementations cannot introduce the same associated name even when their valid substitutions can
be proven disjoint. Such declarations would make ordinary lookup an implicit form of constraint-based overloading.

Callable alternatives must use separately named callables and an explicit callable overload family. The overload family occupies
its shared ordinary name once.

A conflict is rejected and retains every conflicting declaration for diagnostics. Lookup does not select the first declaration.

### Generic applicability

Members are aggregated at the named type-definition level. A generic inherent member is a declaration template associated with that
definition.

For a constructed type, member applicability substitutes the type arguments, matches the inherent implementation subject, and
proves the implementation's static constraints.

If those constraints are not proven, the member is not applicable to that constructed type. In generic code, the surrounding static
constraints must establish the member's implementation constraints before the member can be used.

An inapplicable declaration still reserves its ordinary name in the type-definition surface. Another constrained implementation
cannot reuse that name as an alternative.

Name lookup distinguishes an absent member from an inaccessible member, a member of the wrong semantic category, a member whose
constraints are not satisfied, a malformed member, and a conflicting member set.

### Lifecycle slots

The primary constructor, finalizer, destructor, scope enter declaration, and scope exit declaration occupy typed lifecycle slots
rather than ordinary names.

At most one declaration template can occupy each lifecycle slot across the type body and all inherent implementations. Generic
constraints do not create alternative declarations for the same slot.

A named constructor occupies an ordinary name and follows the ordinary associated-name conflict rules.

### Visibility and deterministic order

The associated surface retains both accessible and inaccessible members. Effective reachability is capped by the associated type,
the member's declaring module, and the member's own visibility.

An inherent implementation does not create another visibility boundary. It also does not make a member reachable when its type,
declaring module, or member visibility prevents access.

The deterministic enumeration order is:

1. direct type members in source order,
2. inherent implementations in canonical declaration order,
3. members within each inherent implementation in source order.

This order exists for deterministic metadata, diagnostics, tooling, and tests. It is not semantic lookup precedence.

## `Self` and `self`

`Self` is the implementing subject in trait and implementation contexts.

Inside:

```bray
trait Cloneable
{
    func clone() -> Self;
}
```

`Self` means the subject that implements `Cloneable`.

Inside:

```bray
impl Point(Cloneable)
{
    func clone() -> Self
    {
        ...
    }
}
```

`Self` means `Point`.

`self` is the current receiver inside an instance method body.

```bray
impl Point
{
    func distance_to(pos other: &Self) -> r64
    {
        ...
    }
}
```

The receiver is supplied by method-call syntax.

```bray
point.distance_to(other)
```

`self` is available only inside instance method bodies.

The binding name `self` is reserved for the compiler-introduced receiver.

Static function bodies use `Self` for the implementing subject and receive ordinary parameters through their parameter list.

## Generic traits

A trait can have generic parameters.

Trait generic parameter lists use the same type-parameter and const-parameter syntax as other generic declarations.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

A trait implementation supplies a concrete trait application.

```bray
impl Point(Comparable<Point>)
{
    func compare(pos other: &Point) -> Ordering
    {
        ...
    }
}
```

A generic implementation can satisfy a parameterized set of trait applications.

```bray
impl BufferComparable = Buffer<T>(Comparable<Buffer<T>>)
    with(T: Comparable<T>)
{
    func compare(pos other: &Buffer<T>) -> Ordering
    {
        ...
    }
}
```

Generic implementation parameters are inferred from the implementing subject and trait application.

```bray
impl BufferComparable = Buffer<T>(Comparable<Buffer<T>>)
```

An otherwise unresolved generic name that appears in the implementing subject or trait application becomes an implementation parameter.

If a name resolves to an existing type, constant, or other visible declaration, it is not inferred as an implementation parameter.

The implementation name is written without a generic parameter list.

The implementing subject, trait application, `with(...)` clause, and implementation body can use inferred implementation parameters.

The `with(...)` clause can constrain inferred implementation parameters.

The `with(...)` clause cannot introduce implementation parameters by itself.

Every inferred implementation parameter must appear in the implementing subject or trait application.

An inferred implementation parameter can appear inside an implementation-eligible type-form subject.

```bray
impl VecReadIterable = &Vec<T>(Iterable)
```

Here `T` is inferred from the `&Vec<T>` implementation subject.

This is rejected:

```bray
impl BadCloneBuffer = Buffer<i32>(Cloneable)
    with(T: Cloneable)
{
    ...
}
```

`T` appears only in the `with(...)` clause, so it is not an implementation parameter.

A generic implementation represents a parameterized set of exact trait implementations.

For each valid substitution of the implementation parameters, the implementation produces one exact coherence key:

```text
(SubstitutedImplementingSubject, SubstitutedTraitApplication)
```

A substitution is valid only when it satisfies the implementation’s `with(...)` clause and makes the implementing subject and trait application well formed.

The implementation body is checked once under the implementation’s static constraints.

The body can use only operations, type-valued members, constants, effects, capabilities, and facts established by the implementation’s `with(...)` clause and surrounding declaration context.

Generic implementation overlap is rejected.

Two implementation declarations overlap when some valid substitutions can produce the same exact coherence key.

If the compiler cannot prove that two participating generic implementations are disjoint, they are rejected as overlapping.

Bray does not use specialization ranking between generic implementations.

An implementation is not selected because one implementation’s constraints look more specific than another’s.

If multiple participating generic implementations could produce the same exact coherence key, the program is invalid.

Trait generic parameters are inputs to the trait application.

Trait type-valued members are outputs of the selected trait implementation.

A generic trait should use generic parameters when the caller or constraint site chooses the type relationship.

A generic trait should use type-valued members when the implementation uniquely determines the related type.

For example, `ConvertTo<Target>` uses the target type as an input:

```bray
trait ConvertTo<Target>
{
    consume func convert() -> Target;
}
```

## Iteration traits

Bray iteration is defined by two compiler-known traits:

- `Iterable`, the source-to-cursor contract,
- `Iterator`, the cursor-advance contract.

`Iterator` is the cursor contract:

```bray
trait Iterator
{
    type Element;

    mut func next() -> Element?;
}
```

`Element` is the type produced by each successful step.

`next` advances the cursor.

`next` returns an element value when the cursor produces an element.

`next` returns `none` when the cursor reaches natural exhaustion.

After a cursor returns `none`, later calls to `next` on the same cursor must return `none`.

An iterator cursor can mutate its own state while advancing.

An iterator that yields borrowed elements must preserve Bray aliasing and borrowing rules for every live yielded element.

If a yielded borrow is still live and the compiler cannot prove that advancing the cursor is compatible with that borrow, the next
advance is rejected.

`Iterable` is the source contract:

```bray
trait Iterable
{
    type Element;
    type Cursor;

    consume func iterate() -> Cursor;
}
```

`Element` is the type produced by iteration.

`Cursor` is the cursor type returned by `iterate`.

An `Iterable` implementation is valid only when its selected `Cursor` type satisfies `Iterator` and the cursor element type matches
the iterable element type:

```text
Cursor: Iterator
Cursor(Iterator).Element == Element
```

This is a compiler-known validity rule for `Iterable` implementations.

It is checked as part of trait implementation checking.

`iterate` consumes the implementation subject.

For an owned subject, this consumes the source value into the cursor.

For a borrow subject such as `&T` or `&mut T`, this consumes the borrow value into the cursor, not the borrowed storage.

The cursor can carry the dependency contract of that borrow.

The cursor cannot outlive or extend the source access it depends on.

`Iterable` and `Iterator` define element production.

They do not require separate cardinality, boundedness, or finiteness members.

Cardinality, boundedness, finiteness, iteration order, and element-borrowing facts are established by the selected implementation
contracts, source type facts, cursor contracts, and compiler-known declarations.

An expression form that requires one of those facts is rejected when the fact is not established in the checking context.

For example:

```bray
impl VecReadIterable = &Vec<T>(Iterable)
{
    type Element = &T;
    type Cursor = VecReadCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}

impl VecWriteIterable = &mut Vec<T>(Iterable)
{
    type Element = &mut T;
    type Cursor = VecWriteCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}

impl VecMoveIterable = Vec<T>(Iterable)
{
    type Element = T;
    type Cursor = VecMoveCursor<T>;

    consume func iterate() -> Cursor
    {
        ...
    }
}
```

These three implementations satisfy different exact implementing subjects.

The `for`, `each`, `all`, and `any` expression forms use `Iterable` and `Iterator` through their expression-specific iteration
source rules.

## Trait implementation overload families

Multiple applications of the same generic trait for the same implementing subject are an implementation overload family.

Implementation overload families are explicit.

Different trait applications do not automatically form an implementation overload family.

When an implementing subject needs multiple applications of the same generic trait, each application is declared as a named implementation, and an overload declaration groups those implementation names under the shared subject and trait surface.

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
        ...
    }
}

impl BufferLinesReader = Buffer(Reader<Lines>)
{
    type Element = Line;

    mut func read_next() -> Element?
    {
        ...
    }
}

overload Buffer(Reader) =
{
    BufferBytesReader,
    BufferLinesReader,
}
```

The overload declaration has this form:

```bray
overload SubjectType(TraitName) =
{
    ImplementationName,
}

overload (&SubjectType)(TraitName) =
{
    ImplementationName,
}

overload (&mut SubjectType)(TraitName) =
{
    ImplementationName,
}
```

`SubjectType` is the shared named type subject.

For a generic named type subject, the overload header names the shared subject type declaration.

For an implementation-eligible type-form subject, the overload header names the exact shared type-form subject.

The grouped subject form is required for implementation-eligible type-form subjects in overload headers.

`TraitName` is the trait declaration whose applications are being grouped.

`ImplementationName` names a previously declared named trait implementation.

Each listed implementation must implement the same implementing subject and an application of the named trait declaration.

The overload declaration does not implement the trait.

It maps existing implementation identities to a shared subject and trait surface.

The overload name is the shared surface.

Each overload arm keeps its own implementation name.

Unnamed trait implementations cannot be listed in an implementation overload family.

An implementation overload family can list generic implementation declarations.

The family lists implementation declaration names.

It does not list instantiated implementation arms.

```bray
impl BufferValuesReader = Buffer<T>(Reader<Values>)
{
    type Element = T;

    mut func read_next() -> Element?
    {
        ...
    }
}

impl BufferIndexesReader = Buffer<T>(Reader<Indexes>)
{
    type Element = usize;

    mut func read_next() -> Element?
    {
        ...
    }
}

overload Buffer(Reader) =
{
    BufferValuesReader,
    BufferIndexesReader,
}
```

For generic implementation arms, overlap checking is performed on the exact coherence keys produced by valid substitutions of each arm.

Every possible exact coherence key produced by one arm must be disjoint from every possible exact coherence key produced by every other arm in the same implementation overload family.

This is rejected:

```bray
impl BufferCloneItemsReader = Buffer<T>(Reader<Items>)
    with(T: Cloneable)
{
    ...
}

impl BufferCopyItemsReader = Buffer<T>(Reader<Items>)
    with(T: Copyable)
{
    ...
}

overload Buffer(Reader) =
{
    BufferCloneItemsReader,
    BufferCopyItemsReader,
}
```

A type can satisfy both `Cloneable` and `Copyable`, so both arms can produce the same exact coherence key:

```text
(Buffer<T>, Reader<Items>)
```

The overload family is invalid.

Bray does not use specialization ranking between implementation overload arms.

An implementation arm is not selected because its constraints look more specific than another arm’s constraints.

Generic trait arguments are part of the exact trait application.

Therefore, these are different implementation keys and can coexist when grouped:

```bray
impl BufferBytesReader = Buffer(Reader<Bytes>)
impl BufferLinesReader = Buffer(Reader<Lines>)
```

The same exact implementation key cannot appear more than once:

```bray
impl BufferBytesReader = Buffer(Reader<Bytes>)
impl BufferOtherBytesReader = Buffer(Reader<Bytes>) // invalid
```

If more than one participating implementation for the same implementing subject and generic trait declaration exists in a coherence domain, those implementations must be named and must be grouped by an implementation overload declaration.

A concrete non-overloaded trait implementation can use the unnamed `impl ImplementingSubject(TraitApplication)` form.

A generic trait implementation is a named implementation declaration with inferred generic parameters.

Inferred implementation parameters are type parameters or const parameters that appear in the implementing subject or trait application.

The `with(...)` clause can constrain inferred implementation parameters, but it cannot introduce them.

Method resolution through an implementation overload family follows overload resolution principles.

Receiver mode, receiver compatibility, member name, and explicitly supplied method arguments can select an overload arm.

Result type, expected type, and type-valued member outputs do not select an overload arm.

If no arm matches, the call is rejected.

If more than one arm matches, the call is rejected as ambiguous.

An exact trait application can be selected explicitly with a trait-qualified receiver expression:

```bray
buffer(Reader<Bytes>).read_next()
```

This selects the `Reader<Bytes>` implementation for the receiver before method lookup.

Trait-qualified receiver expression rules are defined in [Path expressions](../expressions/path-expressions.md#method-paths-and-method-calls).

## Trait use in constraints

Traits participate in generic constraints.

Generic constraints are written with a `with(...)` clause.

A `with(...)` clause is a static predicate-expression context.

Its entries are comma-separated static predicate expressions.

Static predicate expression rules belong to the contract and trust rules.

A static predicate expression can require that an implementing subject satisfy a trait application:

```bray
func max<T>(left: T, right: T) -> T
    with(T: Comparable<T>)
{
    ...
}
```

The left side of a trait satisfaction constraint is an implementing subject.

It can be a named type subject, a generic parameter used as a type subject, or an implementation-eligible type-form subject.

```bray
with(&T: Iterable)
with(&mut T: Iterable)
```

The trait application in a trait satisfaction constraint must be exact.

```bray
with(T: Comparable<T>)
```

If the trait declaration is generic, the constraint must supply the generic arguments required by that trait application.

If the trait declaration is not generic, the trait name alone is the exact trait application.

```bray
with(I: Iterator)
```

When a qualified member reference uses a constrained implementation-eligible type-form subject, the subject is grouped:

```bray
(&T)(Iterable).Element
(&mut T)(Iterable).Cursor
```

Static predicate expressions can also state type equality.

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I: Iterator,
        I(Iterator).Element == Token,
    )
{
    return iter.next();
}
```

Type equality can relate type-valued members from different constrained types:

```bray
func zip_same<A, B>(left: A, right: B)
    with(
        A: Iterator,
        B: Iterator,
        A(Iterator).Element == B(Iterator).Element,
    )
{
    ...
}
```

Constraint facts are unordered.

The type-valued member equality can appear before or after the trait satisfaction constraint that makes the qualified reference valid.

The same constraint set must establish the exact trait application for the type-valued member reference to be accepted.

This is rejected:

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I(Iterator).Element == Token,
    )
{
    ...
}
```

The equality mentions `I(Iterator).Element`, but the constraint set does not establish `I: Iterator`.

Type equality does not introduce a type alias.

Type equality does not select a trait implementation.

Type equality does not choose an arm from an implementation overload family.

Result type and expected type do not infer missing trait satisfaction constraints.

A generic body can use only operations, type-valued members, constants, effects, capabilities, and facts established by its static constraints and by surrounding declaration context.

## Trait method resolution

A method call can resolve to an inherent method or a trait method.

```bray
value.method(argument)
```

Method resolution uses:

- receiver type,
- receiver capability,
- inherent implementations,
- participating trait implementations,
- visible declarations,
- constraints,
- overload rules.

The selected method must match the receiver mode and argument binding supplied by the call.

The selected method must satisfy type checking, ownership checking, borrowing checking, capability checking, effect checking, and contract checking.

A method call resolves to exactly one callable.

Ambiguous method calls are rejected.

When method resolution sees an implementation overload family, resolution uses the same overload principles as callable overloads.

The receiver mode, receiver compatibility, member name, and explicitly supplied method arguments may select one arm.

Result type, expected type, and type-valued member outputs do not select an arm.

A trait-qualified receiver expression can select an exact trait application before member lookup.

Trait implementations for implementation-eligible type-form subjects participate in method resolution only when the receiver expression has that exact receiver type.

Method resolution does not create a shared borrow or mutable borrow solely to search for a type-form implementation.

```bray
values.iterate()        // checks implementations for Vec<T>
(&values).iterate()     // checks implementations for &Vec<T>
(&mut values).iterate() // checks implementations for &mut Vec<T>
```

Expression forms that define their own access mode can create the relevant borrow before method resolution according to that expression form's rules.

Method-call expression rules are defined in [Method call expressions](../expressions/method-call-expressions.md).

## Static function resolution

A static function call can resolve to an inherent static function or trait static function.

```bray
Point.origin()
```

The path before the static function name determines the type, trait application, module, or package context used for resolution.

A static function call has no receiver.

Static function arguments follow the callable's parameter call surface.

Static function call expression rules are defined in [Static function call expressions](../expressions/static-function-call-expressions.md).

## Trait coherence

For a given coherence domain, the exact coherence key for a trait implementation is:

```text
(ImplementingSubject, TraitApplication)
```

Any package can declare a trait implementation for any reachable implementing subject and trait application.

An implementation participates in a coherence domain only when the implementation is declared in that domain or explicitly made visible in it.

A package dependency makes implementation declarations reachable for explicit visibility.

A package dependency does not silently make dependency implementations participate in the dependent coherence domain.

Transitive dependency implementations do not participate unless using declarations or re-exports explicitly make them visible according to module and implementation coherence rules.

For each exact coherence key, Bray requires at most one participating implementation in a coherence domain.

Path qualification can name an implementation declaration.

Path qualification does not bypass coherence checking or activate an implementation for implicit trait satisfaction.

Generic arguments are part of the trait application.

Therefore these implementations have different exact coherence keys:

```bray
impl BufferBytesReader = Buffer(Reader<Bytes>)
impl BufferLinesReader = Buffer(Reader<Lines>)
```

These implementations have the same exact coherence key and are rejected:

```bray
impl BufferBytesReader = Buffer(Reader<Bytes>)
impl BufferOtherBytesReader = Buffer(Reader<Bytes>)
```

The implementing subject is also part of the exact coherence key.

Therefore these implementations have different exact coherence keys:

```bray
impl VecReadIterable = &Vec<T>(Iterable)
impl VecWriteIterable = &mut Vec<T>(Iterable)
impl VecMoveIterable = Vec<T>(Iterable)
```

These implementations have the same exact coherence key and are rejected:

```bray
impl VecReadIterable = &Vec<T>(Iterable)
impl VecOtherReadIterable = &Vec<T>(Iterable)
```

An implementation overload family groups multiple exact coherence keys that share an implementing subject and trait declaration.

It does not allow duplicate exact coherence keys.

For generic implementations, the set of exact coherence keys produced by all valid substitutions must be disjoint from every other participating implementation in the same coherence domain.

Overlapping generic implementations are rejected.

This keeps method resolution, generic checking, and public API compatibility deterministic.

## Trait views and dynamic dispatch

Traits are behavioral contracts.

Traits are not value types.

Using a trait name as a stored type is rejected.

```bray
struct Logger
{
    sinks: [Sink; 4]; // invalid
}
```

Open heterogeneous storage through a trait uses a trait view behind an explicit storage or access type form.

```bray
struct Logger
{
    sinks: [box[Heap] view Sink; 4];
}
```

Dynamic dispatch in Bray is dispatch through a trait view.

It uses the implementation witness carried by the view.

It does not perform structural method lookup at runtime.

It does not search for methods by name at runtime.

It does not expose the hidden concrete type.

Generic constraints and trait views are separate forms of polymorphism.

A generic constraint keeps the concrete type known to the generic instantiation.

```bray
func write_all<S>(pos sink: S, pos message: string)
    with(S: Sink)
{
    sink.write(message);
}
```

A trait view hides the concrete type and dispatches through the selected implementation witness.

```bray
func write_one(pos sink: &view Sink, pos message: string)
{
    sink.write(message);
}
```

The trait-view type form, view-surface rules, receiver restrictions, and ownership behavior are defined by the trait-view type form.

## Inherent implementation API compatibility

The reachable type-associated surface is part of a named type's API.

Adding, removing, or changing a public or otherwise reachable inherent member can be an API change.

Changing an inherent member's name, semantic category, generic applicability, effective reachability, callable surface, selected
type or value, lifecycle slot, contract, or behavior can be an API change.

Moving a member between the type body and an inherent implementation changes declaration identity and can affect diagnostics,
documentation, navigation, and interface metadata even when its callable or value surface remains otherwise equivalent.

## Trait API compatibility

A public trait is part of the public API.

Adding a required member to a public trait is a public API change.

Removing a member from a public trait is a public API change.

Changing a member name is a public API change.

Changing a member receiver mode is a public API change.

Changing parameter names or `pos` permissions is a public API change because they change the callable call surface.

Changing parameter types is a public API change.

Changing a result type is a public API change.

Adding a required type-valued member to a public trait is a public API change.

Removing a type-valued member from a public trait is a public API change.

Changing a type-valued member name is a public API change.

Changing a reachable trait implementation's selected type-valued member binding can be a public API change.

Adding a required constant-valued member to a public trait is a public API change.

Removing a constant-valued member from a public trait is a public API change.

Changing a constant-valued member name, declared type, default value, or selected implementation value can be a public API change.

Adding a required predicate member to a public trait is a public API change.

Removing a predicate member from a public trait is a public API change.

Changing a predicate member name, parameters, body, trusted state, or selected implementation predicate can be a public API change.

Adding a lifecycle requirement to a public trait is a public API change.

Removing a lifecycle requirement from a public trait is a public API change.

Changing a lifecycle requirement kind, signature, execution mode, result shape, contract clauses, or required scoped-capability
type can be a public API change.

Changing member contract clauses can be a public API change when requirements, guarantees, effects, capabilities, trusted obligations, or caller-visible behavior change.

Changing a default member body can be a public API change when observable behavior changes for implementations that use the default.

Changing trait visibility is a public API change.

Changing trait implementation reachability can be a public API change when it affects method resolution or generic satisfaction.

Changing implementation overload family membership can be a public API change when it affects trait satisfaction, method resolution, type-valued member selection, predicate member selection, lifecycle requirement satisfaction, or generic satisfaction.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Traits](traits.md)
- Next: [Summary](summary.md)
