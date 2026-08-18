# Traits

A **trait** is a named behavioral contract.

A trait defines behavior that a type can satisfy through an explicit implementation.

Traits are declared with `trait`.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A trait declaration introduces a named compile-time entity.

A trait is used by implementations, constraints, method resolution, callable checking, and public API compatibility.

A trait can be generic.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

A generic trait produces trait applications when supplied with generic arguments.

```bray
Comparable<Point>
```

A trait application is the trait plus its supplied arguments.

A trait application is part of the behavioral contract that an implementation satisfies.

## Trait visibility

A trait declaration can be `public` or `internal`.

`public` is the default.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}

internal trait ParserReports
{
    func report_state() -> ParserState;
}
```

A public trait exposes its full contract surface as public API.

An internal trait is available within its intended scope.

Use of an internal trait outside its intended scope requires explicit internal-use acknowledgement.

Trait members inherit the visibility of the trait.

The grammar excludes `public` and `internal` modifiers on individual trait members.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

The full trait body is the contract surface of the trait.

## Trait member declarations

A trait body contains member declarations that make up the trait’s behavioral contract.

The trait member forms are:

- callable member declarations,
- constant-valued member declarations,
- type-valued member declarations,
- predicate member declarations,
- lifecycle requirement declarations.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A trait callable member can be required or defaulted.

A required callable trait member has no body and ends with a semicolon.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A defaulted callable trait member has a body.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;

    func not_equals(pos other: &Other) -> bool
    {
        ...
    }
}
```

A defaulted member body provides default behavior for implementations that do not supply that member.

A defaulted member body is checked in trait context.

A defaulted callable member body remains an executable body. It is not declaration-surface completion merely because it
provides default behavior. The declaration surface records the body's presence and selection role, while ordinary
callable body checking owns the checked body.

A defaulted member body can use the trait’s declared surface, `self` when the member is an instance method, `Self`,
trait parameters, type-valued members, constant-valued members, predicate members, available constraints, and
declarations visible from the trait declaration context.

A defaulted member body must satisfy the member's declared result type, ownership behavior, borrowing behavior, and
callable contract.

## Qualified trait member references

Outside the declaring trait and an implementation of that trait, implementation-selected trait members are referenced
through a qualified trait member reference.

Value and type member references use this form:

```bray
SubjectType(TraitApplication).MemberName
```

Predicate member references use this form:

```bray
SubjectType(TraitApplication).predicate_name(...)
```

`SubjectType` is the implementing subject whose selected member is being referenced.

`TraitApplication` is the trait application that declares the member.

`MemberName` is the selected constant-valued or type-valued member declared by that trait.

When the subject is an implementation-eligible type-form subject, the subject must be grouped before the trait
application:

```bray
(&SubjectType)(TraitApplication).MemberName
(&mut SubjectType)(TraitApplication).MemberName
```

For predicate member references, the same grouping applies before the predicate call:

```bray
(&SubjectType)(TraitApplication).predicate_name(...)
(&mut SubjectType)(TraitApplication).predicate_name(...)
```

The trait application in a qualified trait member reference must be exact.

The qualified reference is valid only when the referenced implementation is available in the checking context.

If no matching implementation is available, the reference is rejected.

If more than one matching implementation is available in the relevant coherence domain, the reference is rejected as
ambiguous.

Visibility and internal-use acknowledgement rules apply to the trait application, implementation, and selected member.

## Constant-valued members in traits

A constant-valued member is a compile-time value output of a trait implementation.

```bray
trait HasCapacity
{
    const CAPACITY: usize;
}
```

A constant-valued member declaration without an initializer introduces a required constant-valued member.

A trait implementation must provide every required constant-valued member.

```bray
impl PacketBuffer(HasCapacity)
{
    const CAPACITY: usize = 1500;
}
```

A constant-valued member declaration with an initializer provides default behavior.

```bray
trait Chunked
{
    const CHUNK_SIZE: usize = 4096;
}
```

An implementation can provide a constant-valued member that overrides the default value for that implementation.

When an implementation omits a defaulted constant-valued member, the trait's default value is used for that
implementation.

The constant-valued member declaration syntax is the ordinary constant declaration syntax, except that a required
constant-valued member omits the initializer:

```text
const identifier ':' type-expression ['=' constant-expression] ';'
```

The type annotation is required.

The initializer, when present, is checked in constant-initializer context.

The initializer is a declaration-owned constant definition template. Its selected concrete value is evaluated after the
exact implementing subject, trait application, implementation, generic substitution, and target properties are known.

The initializer must be compatible with the declared constant type.

Inside the declaring trait, the constant-valued member name is available in member signatures, default bodies, and
contract clauses.

Inside an implementation of the trait, the constant-valued member name refers to the value selected by that
implementation.

A constant-valued member name must be unique among the trait's member names.

A constant-valued member cannot be declared `mut`.

A constant-valued member cannot have its own generic parameters.

For a given participating implementation of a concrete trait application, every constant-valued member has exactly one
selected value.

The selected value can depend on the implementing subject and on the trait application's generic arguments.

The selected value cannot depend on a runtime value, control-flow path, local inference choice, caller preference, or
use site.

Outside the declaring trait and an implementation of that trait, a constant-valued member is referenced with
[qualified trait member reference](#qualified-trait-member-references) syntax.

If the implementing subject has multiple visible implementations that could provide different selected constant values,
the reference is rejected by implementation coherence before expression checking.

A constant-valued member reference is a compile-time constant expression when the selected value is valid in the current
constant-expression context.

Changing a constant-valued member value can change the public contract of an implementation.

## Type-valued members in traits

A trait type-valued member is a type-level output of a trait implementation.

Required type-valued member declarations are allowed only in trait declarations.

```bray
trait Iterator
{
    type Element;

    mut func next() -> Element?;
}
```

A trait type-valued member declaration without a binding introduces a required type member.

Inside the declaring trait, the type-valued member name is available in that trait’s member signatures, default bodies,
and contract clauses.

Trait type-valued members are immutable.

A trait type-valued member name must be unique among the trait's member names.

A trait type-valued member cannot be declared `mut`.

A trait type-valued member cannot be rebound after the implementation has selected its value.

A trait type-valued member is not a type alias.

A trait type-valued member does not introduce an alternate name for an arbitrary type outside the trait relationship
that defines it.

Module-level type aliases are not part of Bray.

Structs, unions, modules, packages, and functions cannot declare required type-valued members.

Inherent implementations can bind type-valued members for their implementation subject, but those bindings are
type-associated members rather than trait requirements.

Trait parameters and type-valued members have different roles:

- trait parameters are inputs to a trait application,
- type-valued members are outputs selected by the implementation of a trait application.

For a given participating implementation of a concrete trait application, every type-valued member has exactly one
selected type.

The selected type can depend on the implementing subject and on the trait application’s generic arguments.

The selected type cannot depend on a runtime value, control-flow path, local inference choice, caller preference, or use
site.

An implementation of a trait with required type-valued members must bind each required type member explicitly.

```bray
impl TokenCursor(Iterator)
{
    type Element = Token;

    mut func next() -> Element?
    {
        ...
    }
}
```

The `type Element = Token;` implementation member is a trait-member binding.

It is not a general type alias declaration.

Inside an implementation of the trait, the type-valued member name refers to the selected type bound by that
implementation.

An implementation member that refers to a type-valued member is checked after substituting the implementation’s selected
type.

A trait implementation satisfies the trait only when its callable members match the trait contract after all type-valued
member bindings are applied.

Type-valued members participate in type checking, callable checking, method resolution, generic constraints, contract
checking, flow-sensitive contract checking, documentation, and public API compatibility.

Type-valued members do not create runtime type identity.

Type-valued members do not permit downcasting, runtime type tests, or dynamic type mutation.

Dynamic dispatch through a trait view is rejected when it would hide selected type-valued members that are visible
through that dispatch surface.

Trait type-valued members cannot have their own generic parameters.

```bray
trait StreamingParser
{
    type Output;       // valid
    type Token<T>;     // invalid
}
```

Trait type-valued member defaults are not allowed.

```bray
trait Parser
{
    type Error = ParseError; // invalid
}
```

Changing a type-valued member binding can change the public contract of the implementation.

Outside the declaring trait and an implementation of that trait, a trait type-valued member is referenced with
[qualified trait member reference](#qualified-trait-member-references) syntax:

```bray
SubjectType(TraitApplication).MemberName
```

Grouped subject syntax is required for implementation-eligible type-form subjects:

```bray
(&Vec<T>)(Iterable).Cursor
(&mut Vec<T>)(Iterable).Element
```

For example:

```bray
TokenCursor(Iterator).Element
```

A generic type parameter can be the subject when the surrounding constraints require the trait application:

```bray
func first<I>(iter: I) -> I(Iterator).Element?
    with(I: Iterator)
{
    return iter.next();
}
```

The result type is the `Element` selected by the participating implementation of `Iterator` for `I`.

Trait applications with generic arguments are written inside the parentheses:

```bray
Buffer(JsonEncode<Compact>).Output
Buffer(JsonEncode<Pretty>).Output
```

An implementation overload family header is not an exact trait application reference when it names several applications.

```bray
Buffer(Reader<Bytes>).Element
Buffer(Reader<Lines>).Element
```

The trait application is required outside the declaring trait and its implementation.

```bray
I.Element             // invalid
I(Iterator).Element   // valid
```

The unqualified member name is available only inside the declaring trait and inside an implementation of that trait.

A qualified type-valued member reference is a type expression.

It does not access a runtime field or value member.

## Predicate members in traits

A predicate member is a contract-level relation declared by a trait.

```bray
trait Buffer
{
    predicate valid(value: &Self);

    predicate has_space(value: &Self, count: usize) =
        value.length() + count <= value.capacity();
}
```

A predicate member declaration without a body introduces a required predicate member.

A predicate member declaration with `=` and a predicate expression defines the predicate for every implementation of the
trait.

Trait-defined predicate members are not overridden by implementations.

An implementation must provide every required predicate member.

```bray
impl PacketBuffer(Buffer)
{
    predicate valid(value: &Self) =
        value.length <= value.capacity;
}
```

An implementation predicate member must match a predicate member declared by the implemented trait.

An implementation cannot provide the same predicate member more than once.

An implementation cannot provide extra predicate members that are not declared by the trait.

A predicate member name must be unique among the trait's member names.

Predicate member parameters can mention `Self`, trait parameters, type-valued members, and ordinary types visible from
the trait declaration context.

A predicate member body is checked as a predicate expression.

Inside the declaring trait, the predicate member name is available in member signatures, default bodies, predicate
bodies, and contract clauses.

Inside an implementation of the trait, the predicate member name refers to the predicate selected by that
implementation.

Outside the declaring trait and an implementation of that trait, a predicate member is referenced with
[qualified trait member reference](#qualified-trait-member-references) syntax:

```bray
SubjectType(TraitApplication).predicate_name(...)
```

Predicate members participate in contract checking, static constraints, flow-sensitive contract checking, generic
satisfaction, documentation, and public API compatibility.

Trusted predicate members follow the ordinary trusted predicate and trusted obligation rules from the contract and trust
rules.

## Lifecycle requirements in traits

A lifecycle requirement is a trait member that requires compatible lifecycle behavior from the implementing subject or
exact trait implementation.

Lifecycle requirements are written with lifecycle declaration syntax and end with `;`.

```bray
trait Scoped<Lease>
{
    enter() -> Result<Lease, LockError>;
    exit(scoped: Lease) -> unit;
}
```

Lifecycle requirements do not have bodies in trait declarations.

The lifecycle requirements allowed in traits are:

- `finalize`,
- `destruct`,
- `enter`,
- `exit`.

Constructor requirements are expressed as static callable members that return `Self` or `Result<Self, E>`.

```bray
trait Openable
{
    static func open(pos path: Path) -> Result<Self, OpenError>;
}
```

A `finalize` requirement is satisfied by compatible finalization behavior on the implementing subject.

```bray
trait Finalizable
{
    async finalize() -> Result<unit, FileError>;
}

struct File
{
    handle: OsHandle;

    async finalize() -> Result<unit, FileError>
    {
        ...
    }
}

impl File(Finalizable)
{
}
```

A `destruct` requirement is satisfied by compatible destruction behavior on the implementing subject.

```bray
trait Destructible
{
    destruct();
}
```

Trait implementations cannot provide `finalize` or `destruct` bodies.

`finalize` and `destruct` remain lifecycle behavior of the concrete subject.

Finalization and destruction do not depend on which trait view or trait implementation is used to observe a value.

For named type subjects, this behavior is provided by the type's type-wide lifecycle declaration.

For implementation-eligible type-form subjects, this behavior is the lifecycle behavior produced by the type form.

Multiple traits can require `finalize` or `destruct` from the same implementing subject.

The implementing subject's lifecycle behavior must satisfy every participating `finalize` or `destruct` requirement.

A `destruct` requirement must be synchronous, infallible, and return `unit`.

If the result type is omitted from a `destruct` requirement, `unit` is inferred.

A type-wide destructor satisfies a `destruct` requirement when it satisfies the requirement's lifecycle signature and
contract clauses.

A `finalize` requirement can be synchronous or asynchronous.

A `finalize` requirement can return `unit` or `Result<unit, E>`.

The implementing subject's finalization behavior must match the requirement's execution mode and result shape.

For a `Result<unit, E>` requirement, the finalizer error type must be compatible with `E`.

The type-wide finalizer must satisfy the requirement's contract clauses.

An `enter` requirement must be paired with a matching `exit` requirement in the same trait.

An `exit` requirement is valid only when the same trait declares the matching `enter` requirement.

The `exit` scoped-capability parameter type must match the successful scoped-capability type of the matching `enter`
requirement after type-valued member bindings have been applied.

An `enter` or `exit` requirement can be satisfied by compatible lifecycle behavior on the implementing subject.

An `enter` or `exit` requirement can also be satisfied by a compatible lifecycle declaration in the trait implementation
body.

```bray
impl File(Scoped<FileLease>)
{
    enter() -> Result<FileLease, LockError>
    {
        ...
    }

    exit(scoped: FileLease) -> unit
    {
        ...
    }
}
```

Trait implementation lifecycle declarations for `enter` and `exit` are selected only after the exact trait
implementation has been selected by ordinary implementation selection rules.

For a given exact implementing subject, exact trait application, lifecycle kind, and lifecycle path, at most one
participating implementation lifecycle declaration can be visible in a coherence domain.

Lifecycle requirements participate in ownership checking, borrowing checking, finalization tracking, with-expression
checking, generic satisfaction, documentation, and public API compatibility.

## Instance methods in traits

Inside a trait body, `func` declares an instance method by default.

```bray
trait Sized
{
    func length() -> usize;
}
```

An instance method has an implicit receiver.

The receiver is not written as a parameter.

The keyword `self` refers to the current receiver inside an instance method body.

The keyword `Self` refers to the implementing subject inside a trait declaration and inside implementations of that
trait.

```bray
trait Cloneable
{
    func clone() -> Self;
}
```

The grammar reserves `self` for the current receiver in instance method bodies.

`self` cannot be declared as an ordinary parameter, local binding, or pattern binding.

An instance method’s ordinary parameters are written inside the parameter list.

Ordinary parameters may still use `Self` as a type when `Self` is in scope.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

The receiver is supplied by method-call syntax.

```bray
point.compare(other)
```

Parameters are named by default.

Parameters marked `pos` can be supplied positionally.

## Receiver modes

The method declaration form determines the receiver mode.

```bray
func length() -> usize;

mut func clear();

consume func into_bytes() -> Bytes;

consume mut func normalize() -> Self;
```

`func` declares an instance method with a shared receiver.

A shared receiver method can observe the receiver.

`mut func` declares an instance method with a mutable receiver.

A mutable receiver method requires mutation authority over the receiver access path when called.

`consume func` declares an instance method that consumes the receiver.

A consuming receiver method requires ownership of the receiver value when called.

After a consuming receiver method call, the old receiver access path is unavailable until reinitialized.

`consume mut func` declares an instance method that consumes the receiver and gives the method body mutable local
authority over `self`.

The receiver mode is part of the member’s callable contract.

An implementation member must use the same receiver mode as the trait member it fulfills.

## Static functions in traits

A static trait function is declared with `static func`.

```bray
trait Parse<T>
{
    static func parse(pos text: string) -> T;
}
```

A static function has no receiver.

`self` is unavailable inside a static function body.

`Self` is available in the static function signature and body.

```bray
trait Empty
{
    static func empty() -> Self;
}
```

Static functions are type-level behavior.

Static functions are opt-in with the `static` modifier because trait callable members default to instance methods.

## Trait contracts

Trait member declarations can have callable contracts.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering
        requires(
            ...
        )
        ensures(
            ...
        );
}
```

`requires(...)` declares preconditions for the member.

`ensures(...)` declares postconditions for the member.

Contract clauses use parenthesized comma-separated lists.

A required trait member’s contract is part of the obligation that implementations must satisfy.

A defaulted trait member’s body is checked against its contract.

An implementation member must satisfy the contract of the trait member it fulfills.

Rules for predicate expressions, flow-sensitive contract reasoning, trusted obligations, and contract clauses belong to
the contract and trust rules.

A required trait member can be trusted.

A trusted required trait member uses the `trusted` modifier and a `uses(...)` clause.

The `uses(...)` clause on a required trait member declares the trusted implementation capability envelope for that
member.

An implementation member that fulfills a trusted required trait member must also be trusted.

The implementation member's `uses(...)` clause must be a subset of the required member's capability envelope.

The implementation member's `uses(...)` clause must still exactly match the trusted capabilities used by that
implementation body.

A trusted required trait member does not by itself impose a trusted caller obligation.

Trusted caller obligations must be declared with `trusted` requirements in `requires(...)` or another caller-visible
contract clause.

Trait member caller-visible effects and capability requirements are represented by the ordinary member surface and
contract clauses.

`requires(...)`, `ensures(...)`, `uses(...)`, lifecycle clauses, cancellation obligations, panic behavior, and async
execution obligations are part of the trait member contract when they affect callers, implementation satisfaction,
generic satisfaction, or public API compatibility.

An implementation member must satisfy the caller-visible contract of the trait member it fulfills.

An implementation member can have stricter internal implementation requirements only when they do not add caller
obligations, weaken guarantees, exceed the trusted capability envelope, or change public behavior.

Generic constraints that require a trait application also require the caller-visible contracts of the trait members used
by the constrained code.

## Conversion traits

Plain conversion expression syntax is implemented through compiler-known traits.

The language substrate declares the conversion trait surface.

A type enables a conversion by satisfying the corresponding compiler-known trait application.

Source code does not declare new `as` forms or implicit conversion behavior.

Plain conversion uses `ConvertTo<Target>`:

```bray
trait ConvertTo<Target>
{
    consume func convert() -> Target;
}
```

`ConvertTo<Target>` is the compiler-known trait application for `as Target`.

`convert` is the member called by a plain conversion expression when no built-in recursive conversion rule applies.

The compiler-known `PlainConversion` operation role identifies this exact trait and callable declaration. A different
trait or callable named `ConvertTo` or `convert` does not participate in plain conversion.

A `ConvertTo<Target>` implementation must be total and value-preserving according to the conversion contract.

Fallible conversion support uses `CheckedConvertTo<Target>`:

```bray
trait CheckedConvertTo<Target>
{
    type Error;

    consume func convert_checked() -> Result<Target, Error>;
}
```

`CheckedConvertTo<Target>` is the compiler-known trait application used by the standard-library
`std.convert<Target>(source)` operation.

`convert_checked` is the member called by `std.convert<Target>(source)` when no built-in fallible conversion rule
applies.

The selected `Error` type becomes the error type of the `std.convert<Target>(source)` result.

An implementation of a conversion trait is an ordinary trait implementation:

```bray
impl PortToU16 = Port(ConvertTo<u16>)
{
    consume func convert() -> u16
    {
        return self.value;
    }
}

impl TextToPort = string(CheckedConvertTo<Port>)
{
    type Error = ParseError;

    consume func convert_checked() -> Result<Port, Error>
    {
        ...
    }
}
```

Conversion trait implementations follow trait implementation coherence.

The exact coherence key is still:

```text
(ImplementingSubject, TraitApplication)
```

For `as Target`, the trait application includes the target type:

```text
(SourceType, ConvertTo<Target>)
```

For `std.convert<Target>(source)`, the trait application includes the target type:

```text
(SourceType, CheckedConvertTo<Target>)
```

If the relevant implementation belongs to an implementation overload family, conversion operation resolution uses the
same implementation overload rules as method calls.

The source type, target type, selected conversion operation, and compiler-known conversion member name can select an
implementation arm.

Result type, expected type, and type-valued member outputs do not select a conversion implementation.

No ranking is performed between conversion implementation candidates.

If no participating implementation matches, the plain conversion expression or fallible conversion operation is
rejected.

If more than one participating implementation remains possible, the plain conversion expression or fallible conversion
operation is rejected as ambiguous.

Plain conversion expressions can use only the public compiler-known conversion trait surface and public participating
implementations in the current coherence domain.

The standard-library `std.convert<Target>(source)` operation follows the same public participation rule for fallible
conversion implementations.

Internal-access acknowledgement does not make an implementation candidate available to a plain conversion expression or
standard-library fallible conversion operation.

Internal conversion behavior can be exposed through named functions or methods when the caller explicitly opts into the
internal declaration according to the internal-access rules.

Conversion trait members consume the receiver value produced by the source expression.

When the source expression produces a borrow value, consuming the borrow value does not consume the borrowed storage.

## Operator traits

Operator syntax is implemented through compiler-known traits.

The language substrate declares the overloadable operator set as part of the compiler-known trait surface.

A type enables an operator by satisfying the corresponding compiler-known trait application.

Source code does not declare new operator symbols, new operator precedence, or mappings from arbitrary functions to
operator tokens.

Each operator form has a closed compiler-known role identifying its exact trait, callable, and associated `Output`
declaration when the protocol has one. Operator selection does not recognize these declarations by their names or
spellings.

The overloadable operators are:

| Operator form | Trait application     | Member            | Result            |
|---------------|-----------------------|-------------------|-------------------|
| infix `+`     | `Add<Rhs>`            | `add`             | selected `Output` |
| infix `-`     | `Subtract<Rhs>`       | `subtract`        | selected `Output` |
| infix `*`     | `Multiply<Rhs>`       | `multiply`        | selected `Output` |
| infix `/`     | `Divide<Rhs>`         | `divide`          | selected `Output` |
| infix `%`     | `Remainder<Rhs>`      | `remainder`       | selected `Output` |
| infix `**`    | `Exponentiate<Rhs>`   | `exponentiate`    | selected `Output` |
| infix `@`     | `MatrixMultiply<Rhs>` | `matrix_multiply` | selected `Output` |
| prefix `-`    | `Negate`              | `negate`          | selected `Output` |
| infix `==`    | `Equatable<Rhs>`      | `equals`          | `bool`            |
| infix `!=`    | `Equatable<Rhs>`      | `equals`          | `bool`            |
| infix `<`     | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `<=`    | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `>`     | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `>=`    | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `&`     | `BitAnd<Rhs>`         | `bit_and`         | selected `Output` |
| infix `\|`    | `BitOr<Rhs>`          | `bit_or`          | selected `Output` |
| infix `^`     | `BitXor<Rhs>`         | `bit_xor`         | selected `Output` |
| prefix `~`    | `BitNot`              | `bit_not`         | selected `Output` |
| infix `<<`    | `ShiftLeft<Rhs>`      | `shift_left`      | selected `Output` |
| infix `>>`    | `ShiftRight<Rhs>`     | `shift_right`     | selected `Output` |

The value-producing binary operator traits have this shape:

```bray
trait Add<Rhs>
{
    type Output;

    func add(pos rhs: &Rhs) -> Output;
}
```

`Add<Rhs>` is the compiler-known trait application for binary `+`.

`add` is the member called by the `+` operator.

The other value-producing binary operator traits use the same shared-receiver shape and differ only by trait name,
member name, operator token, and operator meaning.

The unary value-producing operator traits have this shape:

```bray
trait Negate
{
    type Output;

    func negate() -> Output;
}
```

`BitNot` uses the same shape with `bit_not`.

Equality uses `Equatable<Rhs>`:

```bray
trait Equatable<Rhs>
{
    func equals(pos rhs: &Rhs) -> bool;
}
```

`left == right` calls `equals`.

`left != right` is derived from the boolean inverse of `equals`.

`!=` is not implemented separately.

Relational comparison uses `Comparable<Rhs>`:

```bray
trait Comparable<Rhs>
{
    func compare(pos rhs: &Rhs) -> Ordering;
}
```

`Ordering` is the compiler-known comparison result:

```bray
union Ordering
{
    Less;
    Equal;
    Greater;
}
```

`<`, `<=`, `>`, and `>=` are derived from the returned `Ordering`.

The relational operators are not implemented separately.

`MatrixMultiply<Rhs>` is the compiler-known trait for `@`.

`@` is reserved for linear-algebra multiplication semantics, including vector dot product, matrix-vector multiplication,
and matrix-matrix multiplication.

The operator token, fixity, arity, precedence, associativity, trait name, member name, receiver mode, operand parameter
contract, and result rule are part of the compiler-known trait contract.

An implementation of an operator trait is an ordinary trait implementation:

```bray
impl Vec2Add = Vec2(Add<Vec2>)
{
    type Output = Vec2;

    func add(pos rhs: &Vec2) -> Output
    {
        return Vec2 { x = self.x + rhs.x, y = self.y + rhs.y };
    }
}
```

Given participating implementations for the operand types, `left + right` resolves to the `Add<RightType>.add` member
for `LeftType`.

Operator trait members are public language surface.

A unary or binary expression using an overloadable token can use only the public compiler-known operator trait surface
and public participating implementations in the current coherence domain.

Unary and binary expressions using overloadable tokens do not opt into internal access.

Internal-access acknowledgement does not make an implementation candidate available to a unary or binary expression.

Internal implementation details can be used from the named member body, but they are not exposed by the expression
itself.

An operator trait member cannot require mutation authority over the receiver or over caller-provided operand storage.

For receiver-based operators, the operator member uses the shared receiver mode.

An operator trait member cannot be declared with `mut func`, `consume func`, or `consume mut func`.

An explicit operand parameter receives a shared borrow according to the compiler-known trait signature.

An explicit operand parameter cannot require a mutable borrow.

A unary or binary expression using an overloadable token does not consume either operand.

If an operation needs to consume an operand or mutate caller-provided storage, it is expressed as a named method or
function rather than an operator.

Operators not listed in the overloadable operator table are not overloadable.

Assignment, compound assignment, field access, method calls, function calls, indexing, slicing, ranges, borrowing,
nullable propagation, result propagation, panic catching, awaiting, task starting, construction forms, pattern matching,
and lifecycle forms are not operator-overload hooks.

Unary and binary expressions using overloadable tokens follow trait implementation coherence.

The exact coherence key is still:

```text
(ImplementingSubject, TraitApplication)
```

For binary `+`, the trait application includes the right operand type:

```text
(LeftType, Add<RightType>)
```

If the relevant implementation belongs to an implementation overload family, overloadable token resolution uses the same
implementation overload rules as method calls.

Operand types, receiver compatibility, and the operator trait member name can select an implementation arm.

Result type, expected type, and type-valued member outputs do not select an operator implementation.

No ranking is performed between operator implementation candidates.

If no participating implementation matches, the unary or binary expression is rejected.

If more than one participating implementation remains possible, the unary or binary expression is rejected as ambiguous.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Type Forms](type-forms.md)
- Next: [Implementations](implementations.md)
