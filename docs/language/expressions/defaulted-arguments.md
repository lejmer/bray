# Defaulted arguments

Syntax follows the [callable parameters grammar](../syntax-grammar.md#callable-parameters).

A **defaulted argument** is an argument value supplied by a parameter's declared default expression when the call omits
that parameter.

A callable parameter can declare a default value.

```bray
func retry(count: i32 = 3, delay: Duration = Duration.seconds(1))
{
    ...
}
```

A call can omit a parameter that has a default.

```bray
retry();
retry(count = 5);
retry(delay = Duration.seconds(2));
```

A parameter without a default must be supplied by an argument. Duplicate arguments for one parameter and unknown
arguments remain errors even when defaults are available.

Default expressions are [declaration-owned expressions](../declarations/declaration-owned-expressions.md), checked in
their declaration context even when every current call supplies an explicit argument. A default must satisfy the
parameter's type and contract. Checking also enforces ownership, effects, capabilities, finalization obligations, and
flow-sensitive contracts. Trusted implementation capabilities used by a default must be permitted by its owning
declaration.

A parameter default can reference the receiver where applicable, the callable's generic context, declarations visible
from its declaration context, and earlier parameters. It cannot reference itself or later parameters. Call-site local
bindings are unavailable unless supplied through explicit arguments or otherwise available through the callable's
declared context.

Defaults are applied to direct calls after callable selection and do not participate in overload selection. The compiler
checks a used default as part of the call in the same way as an argument supplied by the declaration.

A default is evaluated only when its parameter is omitted. Supplying an explicit argument suppresses evaluation, but
does not suppress declaration checking. An evaluated default's effects become effects of the call. Its finalization
obligations attach to the call result or local temporaries according to ownership rules.
[Call argument evaluation order](expression-evaluation-order.md#call-arguments-and-defaults) defines the ordering of
explicit arguments and omitted defaults.

A default may borrow an earlier by-value argument for use during the call. The callable cannot return a value that
retains that borrow. A borrow supplied by the caller keeps its original lifetime. Values created by defaults follow
ordinary [borrow lifetime rules](../ownership-and-borrowing/borrow-rules.md), including borrows carried inside aggregates
or passed through later defaults.

Struct field defaults and union variant payload defaults follow their own construction-field default rules. They use
the same principle that omitted fields evaluate their defaults as part of construction.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Arguments](arguments.md)
- Next: [Method call expressions](method-call-expressions.md)
