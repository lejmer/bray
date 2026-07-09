# Defaulted arguments

A **defaulted argument** is an omitted parameter value supplied by the parameter’s declared default expression.

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

The omitted parameter receives its declared default expression.

A parameter without a default must be supplied by an argument.

Default expressions are checked in the declaration context where they are written.

They are [declaration-owned expressions](../declarations/declaration-owned-expressions.md) and are checked even when every current call
supplies an explicit argument.

A default expression cannot depend on call-site local bindings unless those bindings are supplied through explicit arguments or otherwise available through the callable’s declared context.

A parameter default can reference the receiver where applicable, the callable's generic context, declarations visible from the
declaration context, and parameters declared before it.

A parameter default cannot reference itself or a later parameter.

A default expression is evaluated when the corresponding argument is omitted.

A defaulted argument participates in the call expression as if the omitted argument expression had been supplied by the declaration.

Defaulted argument evaluation participates in type checking, ownership checking, effects, capability checking, finalization obligations, trusted capability checking, and fact-context behavior.

Effects of a default expression become effects of the call expression when the default is used.

Finalization obligations created by a default expression become obligations of the call expression result or local temporaries according to ownership rules.

Trusted capabilities used by a default expression must be permitted by the declaration that owns the default expression.

A default expression must satisfy the parameter type and contract.

A default expression for a parameter is evaluated only when the parameter is omitted.

Supplying an explicit argument suppresses evaluation of that parameter’s default expression.

It does not suppress declaration checking of the default expression.

Default arguments are applied to direct calls after the callable has been selected.

Default arguments do not participate in overload selection.

Explicit argument expressions are evaluated before omitted parameter defaults.

Explicit argument expressions are evaluated in source order.

Omitted parameter defaults are evaluated after explicit arguments, in parameter declaration order.

Duplicate supplied arguments remain errors even when a parameter has a default.

Unknown supplied arguments remain errors even when other parameters have defaults.

Struct field defaults and union variant payload defaults follow their own construction-field default rules. They use the same principle that omitted fields evaluate their defaults as part of construction.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Arguments](arguments.md)
- Next: [Method call expressions](method-call-expressions.md)
