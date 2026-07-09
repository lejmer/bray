# Declaration-owned expressions

A **declaration-owned expression** is an expression whose meaning forms part of a declaration's callable, construction, constant,
constraint, or contract surface.

Declaration-owned expressions are checked with their declarations even when no use has requested their runtime behavior.

Checking a declaration-owned expression does not by itself execute a runtime expression or lower executable code.

## Declaration-owned expression categories

The declaration-owned expression categories are:

- callable parameter default expressions,
- compiler-known runtime construction default expressions,
- struct field default expressions,
- union payload field default expressions,
- constant initializers,
- defaulted trait constant initializers,
- trait implementation constant-valued member initializers,
- predicate declaration and predicate member bodies,
- `requires(...)`, `ensures(...)`, and `with(...)` predicate expressions,
- other declaration contract expressions defined by their owning declaration forms.

A callable or lifecycle body is not a declaration-owned expression merely because it supplies default behavior. In particular, a
defaulted trait callable body is an executable callable body rather than a declaration-owned default expression.

## Declaration checking and later evaluation

Declaration checking performs the work required to establish that the declaration-owned expression is valid in its declaration
context.

Depending on the expression category, this includes:

- name and member resolution,
- generic substitution validity,
- expected-type and result-type checking,
- ownership and borrowing checking,
- effect and capability checking,
- trusted obligation checking,
- finalization-obligation checking,
- constant-expression validation,
- predicate purity, determinism, totality, and termination checking,
- contract and static-constraint validation.

Later evaluation remains category-specific:

- runtime defaults are evaluated only when the corresponding argument or field is omitted,
- constant definitions are evaluated for concrete constant instances,
- predicate definitions are applied during contract and static-fact reasoning,
- executable callable and lifecycle bodies are evaluated only through their ordinary invocation rules.

An invalid declaration-owned expression makes its declaration invalid even when every current use supplies an explicit value and
would not evaluate that expression at runtime.

## Runtime defaults

Callable parameter defaults, compiler-known runtime construction defaults, struct field defaults, and union payload field defaults are
runtime default expressions.

A runtime default is checked in the declaration context where it is written. It is evaluated in the run that performs the call or
construction only when the corresponding value is omitted.

Supplying an explicit argument or field initializer suppresses runtime evaluation of that default. It does not suppress declaration
checking of the default.

Generic runtime defaults are checked as declaration templates. A use applies the selected generic arguments and implementation
values before evaluating the default.

The checked default surface records the type, generic and contextual dependencies, effects, capabilities, trusted obligations,
ownership and borrowing behavior, and finalization obligations needed to apply the default correctly.

Those requirements become requirements of the call or construction only when the default is used.

### Parameter default dependencies

A callable parameter default can reference:

- generic parameters visible from the callable,
- `Self` and trait-selected members where the callable context provides them,
- the receiver for an instance callable,
- earlier callable parameters,
- declarations visible from the callable's declaration context.

A callable parameter default cannot reference:

- the parameter whose default is being defined,
- a later callable parameter,
- a call-site local binding that is not supplied through the declared input surface,
- runtime state unavailable through the declaration's permitted context.

These rules make default dependencies acyclic and preserve parameter declaration-order evaluation.

Struct field defaults and union payload field defaults cannot reference `self` or sibling fields. Their type-specific chapters define
the remaining construction rules.

## Constant definition templates and instances

A constant initializer is checked as a constant definition template.

A non-generic constant whose dependencies are fully concrete has one constant instance with an empty generic substitution. Checking
that constant includes evaluating its value.

A constant that depends on generic parameters, a trait application, trait-selected members, or target facts can have multiple
concrete constant instances. The compiler checks the definition template under its declared constraints and evaluates each concrete
instance lazily for its exact:

- generic substitution,
- selected implementation values,
- target profile facts.

Definition-level errors belong to the constant declaration. An error that can arise only for one concrete substitution or target
belongs to that constant instance.

A trait constant default is a constant definition template. Its concrete selected value is evaluated after the implementing subject,
trait application, implementation, and required substitutions are known.

Constant initializers are never evaluated as runtime defaults.

## Predicates, constraints, and contracts

A predicate body is checked as a semantic predicate definition rather than evaluated once to one Boolean value.

Predicate bodies, `requires(...)`, `ensures(...)`, `with(...)`, and other declaration contract expressions are checked with their
owning declaration because downstream checking cannot use the declaration correctly without those facts.

Applying a checked predicate to concrete arguments or asking a fact solver to prove it is a separate operation from checking its
definition.

A trusted opaque predicate has no predicate body. Its declared trusted relation and obligations form its declaration surface.

A required trait predicate member has no default predicate definition. A predicate member with a body contributes a checked
predicate definition that can be selected by an implementation according to the trait rules.

## Executable default bodies

A defaulted trait callable body is an executable body.

The trait member's declaration surface records that the body exists and that it supplies default behavior. Checking the executable
body remains ordinary callable body checking and is not required merely to describe the member signature or select the default.

The same separation applies to function, method, constructor, finalizer, destructor, scope-enter, scope-exit, and lambda bodies.

A declaration-owned fact can request an executable body when its own semantics require execution. For example, evaluating a constant
can request the checked body of a const callable invoked by the initializer. That dependency does not reclassify every executable body
as declaration surface.

## Package interfaces and runtime default providers

A reachable runtime default must remain usable without requiring a consuming package to read or rebind the defining package's source
syntax.

Compiled interface metadata therefore records:

- whether the default exists,
- its checked semantic requirements,
- its generic and contextual input surface,
- a stable reference to its compiler-generated runtime default provider.

The provider is not a source-visible declaration and does not participate in ordinary lookup. It evaluates the already checked default
expression when a call or construction omits the corresponding value.

The provider receives permitted receiver, earlier-parameter, generic, trait-selected, and other contextual dependencies through its
compiler-defined input surface. It cannot capture arbitrary call-site locals.

Compiler-known runtime construction defaults use the same semantic provider contract through their compiler-known construction
surfaces.

Provider invocation follows the evaluation order defined by the call or construction expression. Provider lowering and emission are
lazy and occur only when the provider is reachable.

The consuming compiler does not reinterpret the default expression or produce new definition diagnostics for it.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Generic declarations and constraints](generic-declarations-and-constraints.md)
- Next: [Declaration bodies and requirements](declaration-bodies-and-requirements.md)
