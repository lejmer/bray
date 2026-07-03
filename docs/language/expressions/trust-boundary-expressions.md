# Trust boundary expressions

A **trust boundary expression** marks a local trust boundary for one operand expression.

The syntax is:

```bray
trusted expression
```

Example:

```bray
let byte = trusted core.memory.read<u8>(pointer);
```

The operand is checked as the expression covered by the trust boundary.

The trust boundary acknowledges trusted caller obligations required by the operand.

Trust boundary semantics are defined in [Trust boundaries](../contracts-and-trust/trust-boundaries.md).

Trusted obligation propagation is defined in [Obligation propagation](../contracts-and-trust/obligation-propagation.md).

The boundary is explicit source syntax.

It does not perform a runtime check.

It does not prove the trusted facts.

It records that the programmer accepts the trusted caller obligations at that use site.

The trust boundary expression has the same type, value category, ownership result, control-flow behavior, effect behavior, and finalization behavior as its operand.

The boundary scope is exactly the operand expression.

For a single call, the scope is that call expression.

For a block operand, the scope is the block expression.

```bray
trusted
{
    let first = core.memory.read<u8>(first_pointer);
    let second = core.memory.read<u8>(second_pointer);
    yield first + second;
}
```

`trusted expression` does not grant trusted implementation capabilities.

An expression that uses trusted implementation capabilities must still appear inside a trusted declaration with the matching `uses(...)` clause.

`trusted expression` does not bypass visibility, internal-access acknowledgement, ownership checking, borrowing rules, initialization checking, destruction checking, finalization checking, capability checking, effect checking, or ordinary `requires(...)` checking.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Guard expressions](guard-expressions.md)
- Next: [Assertion expressions](assertion-expressions.md)
