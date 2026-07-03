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

If the required trusted facts are already available in the incoming fact context, the boundary records that this operand
depends on them.

If the required trusted facts are not already available, the boundary introduces trusted obligations that must satisfy the
Contract and Trust Model's propagation rules.

The boundary is explicit source syntax.

It does not perform a runtime check.

It does not prove the trusted facts.

It records that the programmer accepts the trusted caller obligations at that use site.

The trust boundary expression has the same type, value category, ownership result, control-flow behavior, effect
behavior, and finalization behavior as its operand.

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

Trusted facts introduced solely by the boundary are available only while checking and evaluating the operand.

They do not enter the surrounding fact context after the trust boundary expression completes.

Facts independently established by the operand's ordinary result, pattern, or `ensures(...)` behavior flow out according to the
ordinary fact-context rules.

A trust boundary expression must acknowledge at least one trusted caller obligation required by its operand.

If the operand has no trusted caller obligation, the boundary is rejected as redundant.

`trusted expression` does not grant trusted implementation capabilities.

An expression that uses trusted implementation capabilities must still appear inside a trusted declaration with the matching
`uses(...)` clause.

`trusted expression` does not bypass visibility, internal-access acknowledgement, ownership checking, borrowing rules,
initialization checking, destruction checking, finalization checking, capability checking, effect checking, or ordinary
`requires(...)` checking.

Inside a callable or lifecycle declaration, trusted obligations introduced by trust boundaries must still satisfy the Contract and
Trust Model's obligation propagation rules.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Guard expressions](guard-expressions.md)
- Next: [Assertion expressions](assertion-expressions.md)
