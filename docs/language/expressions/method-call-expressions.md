# Method call expressions

Call syntax follows the [expression roots](../syntax-grammar.md#expression-roots-and-precedence-structure)
and [argument lists](../syntax-grammar.md#argument-lists) grammar.

A **method call expression** calls an instance-level function through a receiver expression.

```bray
buffer.length()
buffer.clear()
point.distance_to(other)
value.convert<Target>()
```

The expression before the method name is the receiver expression.

The receiver is supplied implicitly by the method call syntax.

An explicit generic argument list appears after the method name and before the call argument list.

Method parameters other than the receiver follow the [argument binding](arguments.md) rules.

Method declaration, receiver-mode, and receiver contract rules are defined in [Methods](../callables/methods.md).

A method call with a positional argument for a non-`pos` parameter is rejected.

```bray
client.connect(timeout)
```

Inside a method body, `self` is the receiver keyword.

`Self` refers to the implementing type inside traits and implementation blocks.

The receiver is not written as an ordinary parameter in method declarations.

The binding name `self` is reserved for the compiler-introduced receiver and cannot be declared as an ordinary
parameter, local binding, or pattern binding.

In trait and implementation contexts, `func` declares an instance method by default.

```bray
func length() -> usize;
```

`func` declares a shared receiver method.

`mut func` declares a mutable receiver method.

```bray
mut func clear();
```

`consume func` declares a consuming receiver method.

```bray
consume func into_bytes() -> Bytes;
```

`consume mut func` declares a consuming receiver method whose method body has mutable local authority over `self`.

## Receiver matching

Receiver matching checks receiver presence, type, and receiver mode during method selection. The receiver type must be
the declared receiver type or a borrow whose target is that exact type. This check does not search for a conversion.

The receiver mode requires the following authority:

| Receiver mode | Required authority |
| --- | --- |
| Shared | Shared observation, mutable access, or ownership. |
| Mutable | Mutable access or mutable ownership. |
| Consuming | Ownership, with or without mutation authority. |
| Consuming mutable | Mutable ownership. |

These checks use the receiver's access capability. They do not prove that its storage is currently initialized, free of
conflicting borrows, or otherwise usable. After selection, the call checks those requirements under
[storage and access paths](../ownership-and-borrowing/storage-and-access-paths.md) and
[borrow rules](../ownership-and-borrowing/borrow-rules.md).

A consuming receiver method makes the receiver's old access path unavailable after the call unless reinitialized.

## Method selection and call checking

A method call resolves through the receiver type and mode, inherent implementations, participating trait
implementations, visible declarations, static constraints, target availability, and overload rules.

Trait implementation participation for implementation-eligible type-form subjects follows
[Trait method resolution](../types/implementations.md#trait-method-resolution).

```bray
values.iterate()        // checks implementations for Vec<T>
(&values).iterate()     // checks implementations for &Vec<T>
(&mut values).iterate() // checks implementations for &mut Vec<T>
```

A method call selects exactly one method through [receiver matching](#receiver-matching), argument mapping,
[argument type matching](arguments.md#argument-type-matching), explicit generic substitution and static constraints,
target availability, and overload resolution. Ordinary call checking then validates ownership, borrowing, mutation
authority, dependency contracts, capabilities, effects, trusted obligations, and contract guarantees for that method.

For overloaded methods, receiver matching, explicit argument mapping and argument type matching, explicit generic
substitution and static constraints, and target availability select the overload arm.

For method calls through a trait implementation overload family, the same inputs and the member name select the
implementation arm.

Result type, expected type, and type-valued member outputs do not select an implementation arm.

If more than one implementation arm remains possible, the method call is rejected as ambiguous.

If the receiver is trait-qualified, the exact trait application is selected before member lookup.

If the receiver access path reaches a trait view, method resolution uses the view's exact trait application.

The method call dispatches through the implementation witness carried by the view.

Dynamic dispatch through a trait view uses ordinary method-call syntax.

There is no separate dynamic-dispatch call syntax.

View formation is checked when an expression is expected to produce a type whose subject is a trait view.

For a borrowed view, the source expression must produce a borrow whose reached concrete type satisfies the exact trait
application.

```bray
let sink: &view Sink = &file_sink;
let sink: &mut view Sink = &mut file_sink;
```

For an owned boxed view, the box construction expression stores the concrete value and records the selected
implementation witness.

```bray
let sink: box[Heap] view Sink = box[Heap](file_sink);
```

The selected implementation must participate in the checking context.

View formation does not permit downcasting, runtime type tests, field access on the hidden concrete type, or calls
outside the view surface.

A synchronous method call produces the method's declared result.

A call to an async method whose declared result is `T` produces an owned `Future<T>`. This rule makes the
compiler-provided `Task<T>.join()` and `Task<T>.cancel()` calls produce `Future<RunResult<T>>` through ordinary
method-call typing.

A synchronous method call makes the method's `ensures(...)` guarantees available after successful completion. An async
method call establishes those conditions only after normal direct-await completion or within the `RunResult.Completed`
arm after task observation. Constructing its `Future<T>` establishes no body postcondition and carries body effects,
capabilities, execution requirements, and lifecycle behavior until execution.

A method call can require ordinary or trusted preconditions through `requires(...)`.

Trusted caller obligations must be present at that program point, acknowledged at a trust boundary, or exposed through
the surrounding declaration’s contract.

The receiver expression is evaluated before method argument expressions.

Method argument expressions and omitted parameter defaults follow the same evaluation-order rules as function calls.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Defaulted arguments](defaulted-arguments.md)
- Next: [Unary and binary expressions](unary-and-binary-expressions.md)
