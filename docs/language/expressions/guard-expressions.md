# Guard expressions

A **guard expression** is an additional boolean check attached to a pattern arm by the expression form that defines the
guard.

Match expressions use `when` guards on `case` arms.

```bray
case pattern when guard
{
    ...
}
```

A guard is evaluated after the arm pattern structurally matches.

A guard is evaluated before the arm body is selected.

A guard is an ordinary expression checked in guard context.

It uses normal expression checking with the capabilities, effects, and conditions available in guard context.

A guard must produce `bool`.

Bindings introduced by the pattern are available in the guard.

Conditions established by successful structural pattern matching are available in the guard.

Surrounding guarantees are available in the guard when they remain valid at the guard evaluation point.

Pattern bindings are available in observe-only form in the guard.

Guard context does not provide consume capability.

Guard context does not provide mutation authority.

Guard context does not allow finalization transfer.

Guard context does not provide trusted capability unless that capability is already available and acknowledged according
to ordinary trust rules.

A guard expression cannot mutate, move, consume, allocate, perform I/O, await, use `return`, use `yield`, use `break`,
use `continue`, use `try`, use `catch`, use `with` expressions, or depend on effects unavailable in guard context.

The arm body is selected only when the pattern matches and the guard evaluates to `true`.

A pattern arm whose pattern matches but whose guard evaluates to `false` does not select that arm.

Arm selection then continues according to the surrounding expression form’s arm-order rules.

For match expressions, the first arm whose pattern matches and whose guard evaluates to `true` is selected.

A guard can establish conditions for the selected arm body when the guard condition is known to hold after evaluation.

Conditions established by a guard remain valid only while the values, storage identities, lifetimes, capabilities, and
versions they depend on remain valid.

Static guard coverage uses the shared condition and predicate system.

For each guarded arm, coverage analysis asks whether the guard condition is statically entailed by:

- conditions established by the arm pattern,
- surrounding conditions still valid at the guard point,
- predicate guarantees available at that program point.

The static result for a guard over a coverage subregion is one of:

- **proven true:** the guarded arm contributes that subregion to exhaustiveness,
- **proven false:** the guarded arm cannot select that subregion,
- **statically unknown:** the guarded arm can select that subregion at runtime, but does not contribute it to
  exhaustiveness.

If a construct requires exhaustive coverage, statically unknown guard coverage does not satisfy that requirement.

If coverage would be complete only by assuming a statically unknown guard, the construct is rejected.

For constructs that do not require exhaustive coverage, a statically unknown guard can produce a warning when it affects
coverage reasoning.

In a consuming match, structural matching and guard evaluation happen by observation first.

Consuming bindings are produced only for the selected arm body after the guard evaluates to `true`.

Guards in a consuming match cannot consume from the subject, move from pattern bindings, or otherwise change the
ownership state that the selected arm body receives.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Predicate expressions](predicate-expressions.md)
- Next: [Trust boundary expressions](trust-boundary-expressions.md)
