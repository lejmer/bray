## Patterns model

A **pattern** is a structural matching form that can refine a value, bind parts of it, and determine how ownership or access paths flow into the matched region.

Patterns are a dedicated grammar category. They use their own binding and checking rules.

Patterns are used by language constructs that decompose values, refine control flow, or introduce bindings from structured data. Pattern contexts include local destructuring, iteration patterns, union handling, and match expressions.

A pattern is checked against a subject type. The subject type determines which pattern forms are valid and what bindings, refinements, ownership state, and fact-context changes the pattern can produce.

---

### Core pattern forms

The core pattern forms are:

```bray
_                         // discard
name                      // resolved name pattern or binding
mut name                  // mutable owned binding
literal                   // literal pattern
Path.Name                 // path pattern
name(...)                 // resolved payload pattern
.name                     // explicit expected-subject no-payload pattern
.name(...)                // explicit expected-subject payload pattern
Type.Variant              // full no-payload variant pattern
Type.Variant(...)         // full payload variant pattern
Type { ... }              // product pattern with explicit type
{ ... }                   // product pattern with expected type
(pattern1, pattern2)      // tuple pattern
[pattern1, pattern2]      // fixed-size array pattern
none                      // nullable absent pattern
?pattern                  // nullable present pattern
box(pattern)              // owned-indirection pattern
pattern1 | pattern2       // alternative pattern
```

Each pattern form preserves the same core rules: structural matching, explicit binding, refutability tracking, ownership-mode checking, and fact-context refinement.

---

### Discard pattern

The discard pattern matches the subject and introduces no binding.

```bray
_
```

The discard pattern is irrefutable.

It is used when a value or part of a value should be matched without being named.

---

### Binding pattern

A bare identifier in a pattern first resolves in pattern context.

```bray
value
```

If the identifier resolves to a pattern-capable declaration, the pattern uses that declaration.

If the identifier does not resolve to a pattern-capable declaration, it introduces a new binding.

Pattern-capable declarations include constants, no-payload variants, payload variants when the pattern uses payload syntax, built-in pattern names, and other declarations that explicitly define pattern behavior.

Functions, ordinary values, modules, and non-pattern declarations are not pattern-capable just because their names are visible.

If pattern resolution is ambiguous, the pattern is rejected.

A binding pattern receives its name only after pattern resolution fails to find a pattern-capable declaration.

This means ordinary binding syntax stays compact while named pattern forms do not require leading punctuation.

Named constants, variants, and other pattern-capable declarations can also be matched through qualified paths.

```bray
Color.Red
Token.EndOfInput
```

---

### Mutable binding pattern

A mutable owned binding pattern uses `mut` before the binding name.

```bray
mut value
```

`mut value` introduces a mutable owned local binding when the pattern operation produces an owned value.

The `mut` applies to the binding introduced by the pattern. It does not change the mutability of the original subject.

The pattern operation determines whether the binding receives an owned value, a copy, or an access path.

---

### Literal patterns

A literal pattern matches a scalar literal value.

```bray
0
1
true
false
'a'
```

Literal patterns are refutable unless the subject type has exactly one possible matching value.

Literal pattern matching uses the language’s literal typing and equality rules for pattern context.

Literal patterns are structural and effect-free.

---

### Path patterns

A path pattern matches a named constant, no-payload variant, or other pattern-capable named declaration.

```bray
Color.Red
Token.EndOfInput
```

A path pattern uses normal Bray path resolution.

A path pattern has no payload bindings unless the resolved declaration is a payload-carrying pattern form.

An unqualified identifier can also resolve to a pattern-capable declaration according to pattern-context name resolution.

---

### Union variant patterns

A union variant pattern matches the active variant of a union value.

Qualified variant pattern:

```bray
Shape.Circle(center = c, radius = r)
```

Expected-subject variant pattern:

```bray
Circle(center = c, radius = r)
```

Explicit expected-subject shorthand:

```bray
.Circle(center = c, radius = r)
```

The unqualified expected-subject form is valid when pattern resolution finds the variant through the subject type or another visible pattern-capable declaration.

The leading-dot form is valid when the expected subject type is a known union type and that union contains the named variant. It is an explicit subject-member shorthand, not the required form.

A no-payload variant pattern uses no parentheses.

```bray
Shape.Empty
Empty
.Empty
```

A payload variant pattern uses parentheses and payload patterns.

```bray
Circle(center = c, radius = r)
```

Named payload patterns are matched by name.

Named payload pattern order does not matter.

A positional payload pattern can match a payload field declared with `pos`.

```bray
Result.Ok(value)
Result.Error(error)
```

Each positional payload pattern supplies the corresponding `pos` payload field by declaration order.

Positional payload patterns must appear before named payload patterns.

A positional payload pattern for a non-`pos` payload field is rejected.

A payload field cannot be matched both positionally and by name.

Duplicate payload fields are errors.

Unknown payload fields are errors.

Missing payload fields are errors unless `..` is present.

Successful matching of a variant pattern refines the subject to that active variant in the matched region.

Payload bindings become available according to the pattern operation’s ownership and access mode.

---

### Product patterns

A product pattern matches a product type such as a `struct`.

Explicit type form:

```bray
Point { x = px, y = py }
```

Expected-type shorthand:

```bray
{ x = px, y = py }
```

Field patterns are matched by name.

Field order does not matter.

Duplicate fields are errors.

Unknown fields are errors.

Missing fields are errors unless `..` is present.

Successful matching of a product pattern makes the selected fields available according to the pattern operation’s ownership and access mode.

---

### Field shorthand

A field pattern can use shorthand when the binding name is the same as the field name.

```bray
{ x, y }
```

This means:

```bray
{ x = x, y = y }
```

The same rule applies to variant payload fields when the pattern entry is not filling a positional payload pattern slot.

```bray
Circle(center, radius)
```

means:

```bray
Circle(center = center, radius = radius)
```

The shorthand introduces bindings with the same names as the matched fields.

Field shorthand is binding shorthand. The introduced field binding is not resolved as a named constant or variant.

For a payload variant with `pos` payload fields, a pattern entry without `=` fills the next positional payload pattern slot while
one is available.

---

### Remaining fields

The `..` pattern explicitly accounts for remaining fields or remaining elements.

```bray
{ x, .. }
Circle(radius, ..)
[first, ..]
[first, .., last]
```

For product and variant payload patterns, `..` accounts for unlisted fields.

For fixed-size array patterns, `..` accounts for unlisted elements.

Omitting fields without `..` is an error.

The `..` pattern introduces no bindings.

---

### Tuple patterns

A tuple pattern matches a tuple by position.

```bray
(x, y)
```

A one-element tuple pattern uses a trailing comma.

```bray
(x,)
```

Tuple pattern arity must match the subject tuple arity.

Each element pattern is checked against the corresponding tuple element type.

A tuple pattern is irrefutable when all element patterns are irrefutable.

---

### Fixed-size array patterns

A fixed-size array pattern matches an array by position.

```bray
[first, second, third]
```

The number of listed element patterns must match the array length unless `..` is present.

```bray
[first, ..]
[first, .., last]
```

Each element pattern is checked against the array element type.

A fixed-size array pattern is irrefutable when it accounts for the array shape and every listed element pattern is irrefutable.

`..` accounts for remaining elements and introduces no binding.

Binding remaining elements requires an explicit slice projection outside the pattern.

---

### Nullable patterns

Nullable patterns match the nullable storage state of a subject with type `T?`.

Absent pattern:

```bray
none
```

Present pattern:

```bray
?inner
```

The `none` pattern is valid only when the subject type is a concrete nullable type `T?`.

It matches the absent state and introduces no binding.

`none` is a built-in nullable pattern and does not introduce a binding named `none`.

The `?inner` pattern is valid only when the subject type is a concrete nullable type `T?`.

It matches the present state and applies `inner` to the contained `T`.

`?inner` is a nullable pattern form, not a general pattern modifier.

Bare binding patterns bind the whole nullable value.

```bray
value   // binds T?
?value  // matches present state and binds contained T
```

Both `none` and `?inner` are refutable.

A nullable pattern set is exhaustive when it covers the absent state and covers the present state for every possible contained `T` value.

```bray
case ?value
{
    ...
}
case none
{
    ...
}
```

This is exhaustive because `value` is irrefutable for the contained `T`.

An alternative pattern can cover both states only when the alternatives bind the same names.

```bray
?_ | none
```

The pattern operation mode determines whether the contained value is observed, borrowed, mutably borrowed, copied, or consumed.

Successful matching of `?inner` refines the subject to present state in the matched region.

Successful matching of `none` refines the subject to absent state in the matched region.

---

### Box patterns

A `box` pattern matches through owned indirection.

```bray
box(inner)
```

A `box(inner)` pattern is valid for a subject of type `box[S] T`.

The inner pattern is checked against `T`.

The pattern operation determines whether matching observes, borrows, mutably borrows, copies, or consumes the contained value.

A consuming `box(inner)` pattern consumes the box and moves through the owned indirection according to `box` ownership rules.

A borrowing `box(inner)` pattern projects a borrow of the contained value according to the storage policy and the `Storage<T>` behavior required by the box type.

---

### Alternative patterns

An alternative pattern matches when any of its alternatives match.

```bray
Error | Cancelled
```

All alternatives are checked against the same subject type.

All alternatives bind the same set of names.

Each shared binding name has the same type and compatible ownership, borrowing, mutation, lifetime, and capability mode in every alternative.

Alternative patterns produce one coherent binding environment for the matched region.

---

### Refutability

A pattern is **irrefutable** when it matches every value of its subject type.

A pattern is **refutable** when it matches only some values of its subject type.

Examples of irrefutable patterns:

```bray
_
value
mut value
{ x, y }
(x, y)
```

A product or tuple pattern is irrefutable when its subpatterns are irrefutable.

Examples of refutable patterns:

```bray
0
true
Circle(radius, ..)
Empty
none
?value
Error | Cancelled
```

A variant pattern is refutable when the subject union has other variants.

A literal pattern is refutable when the subject type has other possible values.

The `none` pattern and `?inner` pattern are refutable because a nullable value can be absent or present.

A pattern context declares whether it accepts refutable patterns.

---

### Pattern contexts

A pattern context is a language construct that applies a pattern to a subject.

Different pattern contexts use different matching modes and refutability rules.

Local destructuring requires an irrefutable pattern.

Iteration patterns require an irrefutable pattern for the iteration element type.

Union handling and match expressions accept refutable patterns and perform coverage checking according to the subject type.

A context that accepts refutable patterns defines what happens when a pattern does not match.

A context that requires irrefutable patterns rejects refutable patterns during checking.

The syntax grammar names this split with `irrefutable-pattern` for irrefutable-only contexts and `case-pattern` for match arms.

The syntax root does not prove refutability by itself. Refutability is checked against the subject type.

---

### Pattern operation modes

A pattern can be applied in different operation modes.

The operation mode is supplied by the construct using the pattern.

The core modes are:

```text
observe
shared borrow
mutable borrow
consume
copy
```

In **observe mode**, the pattern refines the subject and binds observed access paths.

In **shared borrow mode**, the pattern binds shared borrowed access paths.

In **mutable borrow mode**, the pattern binds mutable borrowed access paths when the subject and field contracts permit mutation authority.

In **consume mode**, the pattern moves owned parts out according to ownership rules.

In **copy mode**, the pattern copies matched parts when the type’s copy contract permits it.

The same pattern syntax can be used in multiple operation modes. The surrounding construct decides how the pattern accesses or extracts the matched parts.

---

### Pattern bindings

A binding introduced by a pattern can bind a value or an access path.

The pattern operation mode determines the binding kind.

For example, a binding pattern can introduce:

```text
an owned value,
a copied value,
a shared borrowed access path,
a mutable borrowed access path,
or an observed access path.
```

Every pattern binding has a type, lifetime, capability set, initialization state, and ownership story.

Pattern bindings are scoped to the region introduced by the construct that applied the pattern.

---

### Union refinement

A successful union variant pattern refines the subject to the matched active variant.

Within the matched region:

```text
the active variant is known,
the selected payload exists,
payload fields are initialized,
payload field bindings are available according to the operation mode.
```

A no-payload variant pattern introduces no payload bindings.

A payload variant pattern introduces bindings for the selected payload fields.

Exhaustive handling of a closed union accounts for every variant.

Coverage checking for unions uses the union’s closed variant set.

Control-flow merges after union handling require coherent type, ownership, initialization, destruction, capability, and finalization state.

---

### Partial moves through patterns

A consuming pattern can move fields or payloads out of a subject.

Moving a field or payload is an ownership operation.

Moving parts out requires ownership of the subject and no conflicting active borrows.

After a partial move, the subject is partially initialized.

A partially moved value can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved value destroys only the still-initialized parts.

For unions, destruction follows the active variant and the initialized state of its payload fields.

---

### Fact-context refinement

Successful pattern matching can add facts to the fact context.

Examples of facts established by patterns:

```text
active union variant,
literal equality,
field availability,
payload initialization,
tuple or array shape,
nullable present or absent state,
narrowed control-flow state.
```

These facts are flow-sensitive.

Facts established by a pattern are valid only within the region where the matched state remains valid.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts that depend on the affected value or storage.

---

### Guards

A pattern performs structural matching and binding.

A guard performs an additional boolean check after structural matching.

Guards belong to the surrounding construct that uses the pattern.

The pattern system supplies structural refinement. Guard expressions supply additional condition checking.

Guard coverage and guard proof rules belong to the surrounding expression form and the shared fact and predicate model.

Guard syntax belongs to the match/control-flow model.

---

### Pattern evaluation

Pattern matching is structural, deterministic, and effect-free.

Pattern matching can inspect tags, fields, tuple elements, array elements, nullable state, and type-form structure according to the subject type and operation mode.

Pattern matching can bind names, refine the fact context, and move/copy/borrow parts according to ownership rules.

Ordinary user code is outside pattern matching. User-defined equality, method calls, allocation, I/O, async execution, finalization, and resource scopes belong to surrounding expressions or guards.

---

### Design principles

Patterns are structural matching forms.

Patterns have a dedicated grammar category.

Pattern names resolve before they bind.

Paths identify named constants, variants, and other pattern-capable declarations.

Variant patterns refine closed union values.

Payload and product fields are matched by name.

Field shorthand binds same-name fields without resolving those names as named patterns.

`..` explicitly accounts for remaining fields or elements.

Nullable patterns match absent state with `none` and present state with `?pattern`.

Pattern operation mode determines observe, borrow, mutable borrow, consume, or copy behavior.

Pattern bindings carry type, ownership, lifetime, capability, and initialization state.

Refutability is tracked.

Irrefutable-only contexts reject refutable patterns.

Union handling over closed unions performs exhaustive coverage checking.

Successful patterns refine the fact context.

Pattern matching is structural, deterministic, and effect-free.
