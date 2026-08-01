# Path expressions

A **path expression** is an expression formed by connecting path components with `.`.

```bray
math.sin
point.x
pkg.module.Type
Shape.Circle
pair.0
```

Bray uses `.` for package paths, module paths, type paths, associated declarations, variant access, field access, tuple element
projection, method access, static function access, and nested access.

The binder determines the meaning of each path expression from the resolved left-hand entity and the selected right-hand component.

The left-hand side of `.` can resolve to:

- a package,
- a module,
- a type,
- a trait or trait application,
- a union type,
- a value,
- an access path,
- a box or other type-form value,
- another path-capable compile-time entity.

The result of a path expression can be:

- another path-capable entity,
- a field access path,
- a tuple element access path,
- a callable declaration,
- a callable value,
- a union variant constructor,
- a no-payload union variant value,
- a static function,
- a method candidate,
- a type-level entity,
- a module entity,
- a package entity,
- another compile-time entity.

---

## Package and module paths

A package or module path selects a module path component or declaration from the preceding package or module's ordinary name surface.

```bray
pkg.module.Type
pkg.module.function
math.sin
```

Package and module paths are compile-time paths.

Packages and modules are path lookup providers and declaration containers. They are not separate lookup namespaces in which a name
can coexist with an otherwise conflicting ordinary name.

They do not execute code.

They do not initialize modules.

They do not introduce unqualified names.

A referenced external path must be reachable through the current package or module context, or it must be declared by a `using` declaration.

```bray
using geometry.shapes;

func area(circle: geometry.shapes.Circle) -> r64
{
    ...
}
```

`using` declares that a module, package path, or declaration path is intentionally used by the current module.

`using` participates in dependency checking, visibility checking, internal-use acknowledgement, and tooling.

`using` does not introduce unqualified names.

`using` does not execute code.

`using` does not extend overload sets, conversions, operators, or behavioral contracts except through explicitly referenced paths.

---

## Internal path access

A path that reaches an internal declaration outside its intended scope requires acknowledgement.

Call-site acknowledgement uses `internal`.

```bray
let x = internal some.module.helper();
```

Module-level acknowledgement uses `using internal`.

```bray
using internal some.module.helper;
```

`using internal` acknowledges use of specific internal modules, declarations, or declaration paths.

`using internal` applies to specific modules, declarations, or declaration paths rather than entire packages.

Acknowledgement is lexical.

Acknowledgement does not propagate through re-exports.

Re-exporting an internal declaration requires its own explicit acknowledgement and produces an internal export unless exposed through a public wrapper.

The [module and package rules](../modules-and-packages.md) define export and internal re-export behavior.

A public API exposes internal declarations only through an explicit public wrapper that removes the internal declaration from the public signature.

---

## Type paths

A path whose left-hand side is a type selects a type-associated declaration.

```bray
Point.origin
Buffer.from_bytes
Shape.Circle
ParseResult<i32>.EndOfInput
```

Type-associated declarations include constants, type-valued members, predicates, static functions, named constructors, union
variants, and other declarations associated with the type by type declarations, implementations, or behavioral contracts.

A static function path can be called.

```bray
Point.origin()
```

A union payload variant path can be used as a variant construction expression.

```bray
Shape.Circle(center = origin, radius = 1.0)
```

A no-payload union variant path produces the corresponding union value.

```bray
ParseResult<i32>.EndOfInput
```

No-payload union variant construction uses no parentheses.

A type path can be used in compile-time contexts without producing a runtime value.

---

## Expected-type variant references

A leading-dot variant path refers to a variant of the expected union type.

```bray
.Circle(center = origin, radius = 1.0)
.Empty
```

A leading-dot variant path is valid when the expression context provides a known expected union type and that union contains the named variant.

A leading-dot payload variant path participates in union variant construction.

A leading-dot no-payload variant path produces the no-payload variant value.

An unqualified name can also refer to a variant of the expected union type.

```bray
let circle: Shape = Circle(center = origin, radius = 1.0);
let empty: Shape = Empty;
```

Ordinary lexical value and callable lookup takes precedence over contextual unqualified variant lookup. Contextual lookup is attempted only when ordinary lookup finds no declaration and the expression has a known concrete expected union type.

Contextual lookup does not add variants to lexical scope, search visible unions, infer a union from a variant name, or use the expected type to select overload or implementation candidates.

If the expected union type is absent, the full union path is required.

```bray
Shape.Circle(center = origin, radius = 1.0)
Shape.Empty
```

Expected-type propagation can make leading-dot variant paths available inside type-form construction expressions such as `box(...)`.

```bray
let node: box List<i32> = box(.Empty);
```

Here the expected type `box List<i32>` gives `box(...)` an inner expected type `List<i32>`, and `.Empty` resolves as a variant of `List<i32>`.

---

## Field paths

A path whose left-hand side is a value or access path can select a field.

```bray
point.x
point.y
```

A field path produces an access path to the selected field when the subject expression provides a compatible access path.

Field access observes, borrows, mutably borrows, moves, copies, consumes, or assigns through the selected field according to the operation context.

Observation of a field requires observe capability.

Mutable access to a field requires mutation authority over the reached storage and a field contract that permits mutation.

Fields are immutable by default.

A field declared `mut` can be mutated through a compatible mutable access path.

A mutable binding grants mutation authority over the binding’s access path. Field mutability still controls mutation of the reached field.

Field paths can be chained.

```bray
rectangle.min.x
```

Each component in the chain is checked in order.

The result of each component becomes the left-hand side for the next component.

Field access through a union payload requires active-variant refinement proving that the selected payload exists.

Field access through `box` or another type form follows the access and projection rules of that type form.

---

## Tuple element paths

A path whose left-hand side is a tuple value or tuple access path can select a tuple element by position.

```bray
pair.0
pair.1
```

The right-hand component is a decimal element position.

The element position is checked statically.

The selected position must be within the tuple arity.

A tuple element path produces an access path to the selected element when the subject expression provides a compatible access path.

Tuple element projection observes, borrows, mutably borrows, moves, copies, consumes, or assigns through the selected element
according to the operation context.

Observation of a tuple element requires observe capability.

Mutable access to a tuple element requires mutation authority over the reached storage.

Tuple element paths can be chained with other path components.

```bray
entry.0.name
entry.1.0
```

Tuples do not support bracket indexing or slicing.

```bray
pair[0]     // invalid
pair[0..1]  // invalid
```

Arrays and slices use bracket indexing and slicing. Tuples use dot-number projection.

---

## Method paths and method calls

A path whose left-hand side is a value or access path can select a method candidate.

```bray
buffer.length
buffer.clear
point.distance_to
```

A method call expression calls the selected method candidate.

```bray
buffer.length()
buffer.clear()
point.distance_to(p)
```

The method receiver is supplied by the left-hand expression.

Method resolution is defined in [Method call expressions](method-call-expressions.md).

The method receiver can be qualified by an exact trait application to select a trait implementation before method lookup.

```bray
buffer(Reader<Bytes>).read_next()
```

This is a trait-qualified receiver expression.

It is not a runtime call, cast, conversion, or wrapper construction.

A method path used without a call does not implicitly produce a callable value that captures the receiver.

```bray
let f = buffer.clear; // invalid
```

Use a lambda with an explicit receiver parameter when a callable value should call a method later.

```bray
let clear_buffer = lambda (pos target: &mut Buffer)
{
    target.clear();
};
```

---

## Static paths and static calls

A path whose left-hand side is a type, trait application, module, or package can select a static function.

```bray
Point.origin
Buffer.from_bytes
math.sin
```

A static function path can be called.

```bray
Point.origin()
Buffer.from_bytes(bytes)
math.sin(angle)
```

A static function call has no `self` receiver.

Static functions declared in trait or implementation contexts use `static func`.

```bray
static func origin() -> Point
{
    return Point { x = 0.0, y = 0.0, };
}
```

---

## Path expressions and call syntax

A path expression can be the callee of a call expression.

```bray
math.sin(angle)
Point.origin()
Shape.Circle(center = origin, radius = 1.0)
```

[Argument binding](arguments.md) defines named and positional call arguments.

```bray
add(left = 1, right = 2)
print("hello")
fit(data_frame, max_iterations = 10)
```

Callable-like construction forms use named arguments where field or parameter identity matters.

Struct construction fields use names.

Union variant payload fields use names unless the selected payload field permits positional construction.

Named constructors and static functions use the call surface defined by their parameter declarations.

Tuple expressions and array expressions are structural expressions and use positional element syntax because their element positions are their structure.

---

## Path expressions and assignment

A path expression that produces an assignable access path can be the destination of assignment.

```bray
point.x = 1.0;
counter.value = counter.value + 1;
```

Assignment through a path requires mutation authority over the destination access path.

Assignment through a field path requires the reached field to permit mutation.

Assignment through an active union payload field requires active-variant refinement.

Assignment through a type-form projection requires the type form to permit assignment to the reached storage.

On normal completion, assignment returns `unit`.

Assignment invalidates facts that depend on the previous value or mutated storage.

---

## Path expressions and ownership

A path expression that produces an access path participates in ownership checking.

Observing a path expression preserves ownership.

Borrowing a path expression creates a borrow of the reached storage.

Mutably borrowing a path expression requires mutation authority and compatible exclusivity.

Moving from a path expression moves the reached value when the reached value is owned and the type is not copied instead.

Copying from a path expression requires the reached type to satisfy the copy contract.

Consuming through a path expression requires ownership of the reached value.

Moving a field or payload through a path expression can partially move the containing value.

A partially moved containing value cannot be used as a complete value until reinitialized or consumed by a rule that accounts for its state.

---

## Path expressions and fact context

A path expression can use and refine facts in the fact context.

Examples:

- active union variant facts enable payload field access,
- borrow facts determine whether a path can be borrowed or mutably borrowed,
- initialization facts determine whether a path can be used,
- trusted facts can permit trusted operations reached through paths,
- visibility and internal-use facts determine access to internal declarations.

A path expression can invalidate facts when it is used to move, consume, assign, mutably borrow, destroy, finalize, or otherwise change reached storage.

A path expression that only observes stable storage preserves facts that remain true under observation.

---

## Path resolution failures

A path expression is rejected when resolution fails.

Resolution fails when:

- a path component does not exist on the resolved left-hand entity,
- the component exists but is not visible,
- the component is internal and lacks required acknowledgement,
- the component is ambiguous,
- the selected operation requires capabilities that are unavailable,
- the selected operation requires facts that are absent,
- the selected operation is not valid for the left-hand entity kind,
- the path would expose an internal declaration through a public API without an explicit public wrapper.

Path resolution errors are reported during binding and checking.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Name expressions](name-expressions.md)
- Next: [Index access expressions](index-access-expressions.md)
