# Method call expressions

A **method call expression** calls an instance-level function through a receiver expression.

```bray
buffer.length()
buffer.clear()
point.distance_to(other)
```

The expression before the method name is the receiver expression.

The receiver is supplied implicitly by the method call syntax.

Method parameters other than the receiver follow the [argument binding](arguments.md) rules.

Method declaration, receiver-mode, and receiver contract rules are defined in [Methods](../callables/methods.md).

A method call with a positional argument for a non-`pos` parameter is rejected.

```bray
client.connect(timeout)
```

Inside a method body, `self` is the receiver keyword.

`Self` refers to the implementing type inside traits and implementation blocks.

The receiver is not written as an ordinary parameter in method declarations.

The binding name `self` is reserved for the compiler-introduced receiver and cannot be declared as an ordinary parameter, local
binding, or pattern binding.

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

A method call checks the receiver expression against the method’s receiver mode.

A shared receiver method requires a compatible observable receiver access path.

A mutable receiver method requires mutation authority and compatible exclusivity for the receiver storage.

A consuming receiver method requires ownership of the receiver value.

A consuming receiver method makes the receiver’s old access path unavailable after the call unless reinitialized.

A method call resolves through the receiver type, receiver capability, inherent implementations, participating trait implementations, visible declarations, constraints, and overload rules.

Trait implementation participation for implementation-eligible type-form subjects follows [Trait method resolution](../types/implementations.md#trait-method-resolution).

```bray
values.iterate()        // checks implementations for Vec<T>
(&values).iterate()     // checks implementations for &Vec<T>
(&mut values).iterate() // checks implementations for &mut Vec<T>
```

A method call resolves to exactly one method after receiver checking, argument binding, type checking, ownership checking, capability checking, effect checking, contract checking, and overload resolution.

For overloaded methods, the receiver mode and explicitly supplied method arguments select the overload arm.

For method calls through a trait implementation overload family, receiver mode, receiver compatibility, member name, and explicitly supplied method arguments select the implementation arm.

Result type, expected type, and type-valued member outputs do not select an implementation arm.

If more than one implementation arm remains possible, the method call is rejected as ambiguous.

If the receiver is trait-qualified, the exact trait application is selected before member lookup.

If the receiver access path reaches a trait view, method resolution uses the view's exact trait application.

The method call dispatches through the implementation witness carried by the view.

Dynamic dispatch through a trait view uses ordinary method-call syntax.

There is no separate dynamic-dispatch call syntax.

View formation is checked when an expression is expected to produce a type whose subject is a trait view.

For a borrowed view, the source expression must produce a borrow whose reached concrete type satisfies the exact trait application.

```bray
let sink: &view Sink = &file_sink;
let sink: &mut view Sink = &mut file_sink;
```

For an owned boxed view, the box construction expression stores the concrete value and records the selected implementation witness.

```bray
let sink: box[Heap] view Sink = box[Heap](file_sink);
```

The selected implementation must participate in the checking context.

View formation does not permit downcasting, runtime type tests, field access on the hidden concrete type, or calls outside the view surface.

A method call produces the method’s declared result.

A method call to an async method produces an owned async computation.

A method call can establish facts from the method’s `ensures(...)` clause after successful completion.

A method call can require ordinary or trusted preconditions through `requires(...)`.

Trusted caller obligations must be present in the fact context, acknowledged at a trust boundary, or exposed through the surrounding declaration’s contract.

The receiver expression is evaluated before method argument expressions.

Method argument expressions and omitted parameter defaults follow the same evaluation-order rules as function calls.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Defaulted arguments](defaulted-arguments.md)
- Next: [Unary and binary expressions](unary-and-binary-expressions.md)
