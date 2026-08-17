# Syntax grammar

This chapter explains the Bray EBNF source grammar.

The companion plain EBNF reference is `syntax-grammar.ebnf`.

The lexical grammar is defined separately in `lexical-grammar.md` and `lexical-grammar.ebnf`.

The package product, selected source graph, selected dependency graph, package identity, and target profile are semantic inputs.
They are not source syntax.

---

## EBNF notation

Terminals are written in double quotes.

Non-terminals are written in lowercase words separated by hyphens.

Grammar productions use:

- `=` to define a production,
- `;` to end a production,
- `|` for alternatives,
- `[ ... ]` for optional syntax,
- `{ ... }` for zero or more repetitions,
- `( ... )` for grouping.

Semantic restrictions are not encoded by adding extra grammar branches. For example, rules such as "only one `@test` directive can
apply to a module declaration" are semantic checks.

---

## Compilation unit

A compilation unit is the selected Bray source units for one package product.

```ebnf
compilation-unit =
    { source-unit } ;
```

---

## Source Units

Every source unit starts with explicit module syntax.

A source unit can use one unbraced source-unit module declaration followed by items that belong to that module and then zero or
more braced block module declarations. Alternatively, it can use one or more braced block module declarations.

Loose source-unit items are allowed only after a source-unit module declaration.
Once a block module declaration begins, every remaining top-level declaration is another block module declaration.

```ebnf
source-unit =
      source-unit-module-declaration { module-item } { block-module-declaration }
    | block-module-declaration { block-module-declaration } ;

source-unit-module-declaration =
    module-directives module-modifiers "module" module-path ";" ;
```

---

## Module declarations

```ebnf
block-module-declaration =
    module-directives module-modifiers "module" module-path module-body ;

module-body =
    "{" { module-item } "}" ;

module-item =
      using-declaration
    | export-declaration
    | module-level-declaration ;

module-path =
    path ;
```

Block module declarations are package-level module contributions. They are not nested modules.
They do not inherit from a source-unit module declaration.

---

## Declaration contexts

Declarations are grouped by the source context that accepts them.

Module-level declarations can appear directly after a source-unit module declaration or in a block module body.

```ebnf
module-level-declaration =
      constant-declaration
    | static-declaration
    | function-declaration
    | callable-contract-declaration
    | type-declaration
    | trait-declaration
    | implementation-declaration
    | overload-declaration
    | predicate-declaration ;
```

Function declarations include ordinary function declarations and extern callable declarations.

Conversion behavior is declared through implementation declarations because conversions are defined by satisfying the
compiler-known conversion traits.

Other declaration contexts are named separately and are defined with the grammar for the constructs that contain them.

```ebnf
type-member-declaration =
      type-callable-member-declaration
    | type-constructor-member-declaration
    | type-lifecycle-member-declaration
    | constant-declaration
    | predicate-declaration
    | callable-overload-declaration ;

trait-member-declaration =
      trait-callable-member-declaration
    | trait-constant-member-declaration
    | trait-type-member-declaration
    | trait-predicate-member-declaration
    | trait-lifecycle-requirement-declaration ;

block-level-declaration =
      local-binding-declaration
    | constant-declaration ;
```

Implementation member declarations are defined with implementation declarations.

Inherent and trait implementation bodies share one member grammar. The implementation header determines how those members are
checked.

---

## Module directives

```ebnf
module-directives =
    { module-directive } ;

module-directive =
      target-directive
    | test-directive
    | link-directive ;

directive-marker =
    "@" ;

target-directive =
    directive-marker "target" "(" constant-expression ")" ;

test-directive =
    directive-marker "test" [ "(" "serial" ")" ] ;
```

`@target(...)`, bare `@test`, and `@link(...)` can apply to module declarations. `@test(serial)` applies only to function
declarations.

Directive names are identifier spellings used after `@` in directive context.

Repeated or conflicting directives are rejected by semantic checks.

---

## Module modifiers

```ebnf
module-modifiers =
    [ trusted-modifier ] [ visibility-modifier ] ;

trusted-modifier =
    "trusted" ;

visibility-modifier =
      "public"
    | "internal" ;
```

The required module modifier order is `trusted` before visibility.

---

## Using and export declarations

```ebnf
using-declaration =
    "using" [ "internal" ] path ";" ;

export-declaration =
    "export" path ";" ;
```

---

## Paths

```ebnf
path =
    identifier { "." identifier } ;
```

---

## Expression roots and precedence structure

`expression` accepts every expression form.

Restricted expression roots name contexts that intentionally accept less than a full expression. They let the parser produce a
more precise tree before semantic checking.

The expression grammar is non-left-recursive. Left-associative binary levels use repetition. Right-associative and prefix forms use
right recursion. Postfix forms use repetition.

```ebnf
expression =
    assignment-expression ;

assignment-expression =
    non-assignment-expression [ assignment-continuation ] ;

assignment-continuation =
    assignment-operator expression ;

assignment-operator =
    "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "@=" |
    "&=" | "|=" | "^=" | "<<=" | ">>=" | "**=" ;

non-assignment-expression =
    range-expression ;

range-expression =
    range-bound-expression [ ".." range-bound-expression ] ;

range-bound-expression =
    logical-or-expression ;

condition-expression =
    non-assignment-expression ;

assignable-expression =
    access-expression ;
```

The range level accepts one bounded top-level `..` operator. Its operands are logical-or expressions, so the range operator has
lower precedence than every binary operator and higher precedence than assignment.

Inside square brackets, a top-level `..` selects the slice form. Parentheses make a range expression a grouped element selector.

The expression before an `assignment-continuation` must be an `assignable-expression`. This is a syntactic classification once the
access-expression forms are defined. Name resolution and capability checking still decide whether that syntactic access can
actually be assigned through.

```ebnf
logical-or-expression =
    logical-and-expression { "||" logical-and-expression } ;

logical-and-expression =
    comparison-expression { "&&" comparison-expression } ;

comparison-expression =
    bitwise-or-expression [ comparison-operator bitwise-or-expression ] ;

bitwise-or-expression =
    bitwise-xor-expression { "|" bitwise-xor-expression } ;

bitwise-xor-expression =
    bitwise-and-expression { "^" bitwise-and-expression } ;

bitwise-and-expression =
    shift-expression { "&" shift-expression } ;

shift-expression =
    additive-expression { shift-operator additive-expression } ;

additive-expression =
    multiplicative-expression { additive-operator multiplicative-expression } ;

multiplicative-expression =
    unary-expression { multiplicative-operator unary-expression } ;

unary-expression =
      unary-operator unary-expression
    | exponentiation-expression ;

exponentiation-expression =
    postfix-expression [ "**" unary-expression ] ;

postfix-expression =
    primary-expression { postfix-operation } ;

access-expression =
    access-root-expression { access-postfix-operation } ;

access-root-expression =
      identifier
    | "self"
    | internal-access-expression
    | grouped-access-expression ;

internal-access-expression =
    "internal" access-expression ;

grouped-access-expression =
    "(" access-expression ")" ;

access-postfix-operation =
      member-access-operation
    | element-index-operation
    | generic-argument-list ;

member-access-operation =
    "." member-selector ;

member-selector =
      identifier
    | tuple-element-index ;

element-index-operation =
    "[" range-bound-expression "]" ;

primary-expression =
      literal-expression
    | unit-expression
    | absence-expression
    | access-primary-expression
    | grouped-expression
    | tuple-expression
    | array-expression
    | general-generator-expression
    | expected-type-struct-construction-expression
    | leading-dot-variant-expression
    | block-expression
    | conditional-expression
    | match-expression
    | while-expression
    | for-expression
    | loop-expression
    | with-expression
    | lambda-expression
    | borrow-expression
    | trust-boundary-expression
    | assertion-expression
    | result-propagation-expression
    | catch-expression
    | await-expression
    | type-form-construction-expression
    | boolean-fold-expression
    | yield-expression
    | return-expression
    | panic-expression
    | break-expression
    | continue-expression ;

access-primary-expression =
    access-expression [ struct-construction-body ] ;

postfix-operation =
      access-postfix-operation
    | call-operation
    | slice-index-operation
    | nullable-propagation-operation
    | conversion-operation
    | trait-qualified-member-operation ;

call-operation =
    argument-list ;

slice-index-operation =
    "[" slice-selector "]" ;

slice-selector =
    [ range-bound-expression ] ".." [ range-bound-expression ] ;

nullable-propagation-operation =
    "?" ;

conversion-operation =
    "as" type-expression ;

trait-qualified-member-operation =
    "(" trait-application ")" member-access-operation ;

full-struct-construction-expression =
    access-expression struct-construction-body ;

literal-expression =
      integer-literal
    | real-literal
    | imaginary-literal
    | boolean-literal
    | character-literal
    | string-literal ;

boolean-literal =
      "true"
    | "false" ;

unit-expression =
    "unit" ;

absence-expression =
    "none" ;

grouped-expression =
    "(" expression ")" ;

leading-dot-variant-expression =
    "." identifier ;

borrow-expression =
    "&" [ "mut" ] expression ;

trust-boundary-expression =
    "trusted" expression ;

assertion-expression =
    "assert" "(" condition-expression [ "," expression ] ")" ;

result-propagation-expression =
    "try" expression ;

catch-expression =
    "catch" expression ;

await-expression =
    "await" expression ;

with-expression =
    "with" irrefutable-pattern [ ":" type-expression ] "=" expression
    block-expression ;


yield-expression =
    "yield" [ expression ] ;

return-expression =
    "return" [ expression ] ;

panic-expression =
    "panic" argument-list ;

break-expression =
    "break" [ expression ] ;

continue-expression =
    "continue" ;
```

The parser accepts omitted operands for `yield`, `return`, and `break`. Semantic checking treats an omitted operand as `unit` in
valid target contexts.

The block after a `with` header belongs to the `with-expression`. It is not parsed as part of the initializer expression.

A generic argument list in an access path applies the preceding generic declaration. This form selects closed generic types,
callables, and static instances. A following argument list calls a selected callable. Applying generic arguments to a non-generic
declaration is a semantic error.

### Argument lists

An `argument-list` is the parenthesized runtime argument list used by call expressions and callable-like construction forms.

```ebnf
argument-list =
    "(" [ argument-sequence [ "," ] ] ")" ;

argument-sequence =
      positional-argument-sequence [ "," named-argument-sequence ]
    | named-argument-sequence ;

positional-argument-sequence =
    positional-argument { "," positional-argument } ;

named-argument-sequence =
    named-argument { "," named-argument } ;

positional-argument =
    expression ;

named-argument =
    identifier "=" expression ;
```

The grammar shape allows an empty argument list and a trailing comma after the final supplied argument.

Positional arguments can appear before named arguments. A positional argument cannot appear after a named argument.

There is no empty argument-entry syntax. Omitted parameters are represented by leaving the argument entry out and are checked
against parameter defaults.

When parsing an argument list, an entry beginning with `identifier "="` is a named argument. A positional assignment expression
with that token shape must be grouped.

### Struct construction bodies

A `struct-construction-body` is the braced field-initializer list used by full struct construction and expected-type struct
construction.

```ebnf
struct-construction-body =
    "{" [ struct-field-initializer-sequence [ "," ] ] "}" ;

struct-field-initializer-sequence =
    struct-field-initializer { "," struct-field-initializer } ;

struct-field-initializer =
    identifier "=" expression ;

expected-type-struct-construction-expression =
    struct-construction-body ;
```

The grammar shape allows an empty construction body and a trailing comma after the final supplied field initializer.

Every supplied field initializer names a field explicitly. Struct construction bodies do not have field shorthand syntax.

There is no empty field-initializer syntax. Omitted fields are represented by leaving the field initializer out and are checked
against field defaults.

A non-empty expected-type struct construction body is recognized by the top-level `identifier "="` field-initializer shape. An empty
braced expression is accepted as expected-type struct construction only when the expression context supplies the constructed struct
type. Otherwise it is an empty block expression.

### Type-form construction expressions

A `type-form-construction-expression` is a compiler-recognized construction expression associated with a type form.

```ebnf
type-form-construction-expression =
    box-construction-expression ;

box-construction-expression =
    "box" [ type-form-argument-list ] argument-list ;
```

`box(...)` uses expected type context or the default storage policy.

`box[S](...)` supplies explicit type-form arguments before the runtime construction arguments.

The runtime arguments use `argument-list` syntax. Type-form construction expressions are not ordinary calls. Type-form selection,
produced type, subject type, storage behavior, argument validity, ownership, effects, capabilities, and initialization are semantic
checks.

### Tuple expressions

A `tuple-expression` is a parenthesized expression list that contains a tuple comma.

```ebnf
tuple-expression =
    "(" expression "," [ tuple-expression-tail ] ")" ;

tuple-expression-tail =
    expression { "," expression } [ "," ] ;
```

A one-element tuple uses the trailing comma form `(expression,)`.

Two-or-more-element tuples use comma-separated expressions and can include a trailing comma.

Parentheses around a single expression without a comma are parsed as `grouped-expression`, not `tuple-expression`.

The unit value is spelled `unit`. There is no empty tuple expression syntax.

### Array expressions

An `array-expression` constructs a fixed-size array through supplied elements, repeated elements, or an array generator expression.

```ebnf
array-expression =
      element-array-expression
    | repeated-element-array-expression
    | array-generator-expression ;

element-array-expression =
    "[" array-element-sequence [ "," ] "]" ;

array-element-sequence =
    expression { "," expression } ;

repeated-element-array-expression =
    "[" expression ";" constant-expression "]" ;

array-generator-expression =
    "[" generator-iteration-expression "]" ;
```

An element array expression must contain at least one element expression. Empty array expression syntax is not accepted.

Repeated-element array expressions use a compile-time count expression after `;`.

Array generator syntax is defined with generator expressions.

### Generator expressions

A `general-generator-expression` is a braced generator region whose top-level child is one generator iteration expression.

An `array-generator-expression` is the bracketed array expression form whose child is one generator iteration expression.

```ebnf
general-generator-expression =
    "{" generator-iteration-expression "}" ;

generator-iteration-expression =
    "each" irrefutable-pattern "in" iteration-source block-expression ;
```

`generator-iteration-expression` is the shared `each` form used by general generator expressions, array generator expressions, and
nested generator iteration inside generator bodies.

The source position uses the shared `iteration-source` grammar, including optional `mut` or `move`.

The body is parsed as a block expression in generator-iteration context. Yield handling, element typing, cardinality, finiteness,
ownership, borrowing, effects, and context-specific validity are semantic checks.

### Boolean fold expressions

A `boolean-fold-expression` reduces one operand with `all(...)` or `any(...)`.

```ebnf
boolean-fold-expression =
      all-expression
    | any-expression ;

all-expression =
    "all" "(" boolean-fold-operand ")" ;

any-expression =
    "any" "(" boolean-fold-operand ")" ;

boolean-fold-operand =
    expression ;
```

The operand uses ordinary expression syntax. It can be a generator expression or another finite bounded iterable expression.

`all` and `any` are parsed as boolean fold expressions, not ordinary calls.

Element type, finiteness, boundedness, iteration contracts, and context-specific validity are semantic checks.

### Block expressions

A `block-expression` is a braced sequence of block items.

```ebnf
block-expression =
    "{" { block-item } "}" ;

block-item =
      block-level-declaration
    | generator-iteration-expression
    | block-shaped-expression
    | sequenced-expression ;

block-shaped-expression =
      block-expression
    | conditional-expression
    | match-expression
    | while-expression
    | for-expression
    | loop-expression
    | with-expression ;

sequenced-expression =
    expression ";" ;

local-binding-declaration =
    "let" irrefutable-pattern [ ":" type-expression ] "=" expression ";" ;
```

Block expressions can be empty.

A block-shaped expression is self-delimiting and can appear directly as a block item without a trailing `;`. It may instead use a
trailing `;`, in which case it is a sequenced expression.

Other expression block items are sequenced expressions and always end with `;`. A block expression does not use a final
unterminated expression as its result. Value production is handled by `yield`.

Block-level declarations are local binding declarations and constant declarations.

The pattern in a local binding declaration is checked as an irrefutable pattern.

A brace-enclosed expression whose only top-level child is a `generator-iteration-expression` is a `general-generator-expression`
rather than a `block-expression`.

A generator iteration block item is valid only in a generator-iteration context or as the top-level child of a
`general-generator-expression`.

### Control-flow expressions

Control-flow expressions use header syntax followed by block bodies.

```ebnf
conditional-expression =
    "if" condition-expression block-expression [ "else" conditional-else ] ;

conditional-else =
      block-expression
    | conditional-expression ;

match-expression =
    "match" match-subject match-body ;

match-subject =
    [ "consume" ] expression ;

match-body =
    "{" match-arm { match-arm } "}" ;

match-arm =
    "case" case-pattern [ "when" guard-expression ] block-expression ;

while-expression =
    "while" condition-expression block-expression [ "else" block-expression ] ;

for-expression =
    "for" irrefutable-pattern "in" iteration-source block-expression
    [ "else" block-expression ] ;

iteration-source =
    [ iteration-source-mode ] expression ;

iteration-source-mode =
      "mut"
    | "move" ;

loop-expression =
    "loop" block-expression ;
```

The block after a control-flow header belongs to the control-flow expression. It is not parsed as part of the condition, subject,
or source expression in the header.

`else if` is the conditional-else case that contains another conditional expression.

A match body contains one or more `case` arms. Match arms use block bodies and do not need separators.

The `consume` marker in a match subject selects consuming match mode.

The `mut` and `move` markers in an iteration source select mutable and consuming iteration modes. An unmarked iteration source
selects shared iteration mode.

Only match arms have `when` guards.

### Pattern roots

Pattern grammar is split by context.

`irrefutable-pattern` is used by binding-like positions that require every possible subject value to match.

`case-pattern` is used by match arms and accepts alternatives.

```ebnf
irrefutable-pattern =
    irrefutable-pattern-primary ;

case-pattern =
    case-pattern-primary { "|" case-pattern-primary } ;

irrefutable-pattern-primary =
      shared-pattern-primary
    | irrefutable-recursive-pattern ;

case-pattern-primary =
      shared-pattern-primary
    | case-recursive-pattern ;

shared-pattern-primary =
      discard-pattern
    | mutable-binding-pattern
    | literal-pattern
    | nullable-absent-pattern ;

irrefutable-recursive-pattern =
      irrefutable-nullable-present-pattern
    | irrefutable-box-pattern
    | leading-dot-irrefutable-pattern
    | path-irrefutable-pattern
    | expected-type-irrefutable-product-pattern
    | grouped-irrefutable-pattern
    | irrefutable-tuple-pattern
    | irrefutable-array-pattern ;

case-recursive-pattern =
      case-nullable-present-pattern
    | case-box-pattern
    | leading-dot-case-pattern
    | path-case-pattern
    | expected-type-case-product-pattern
    | grouped-case-pattern
    | case-tuple-pattern
    | case-array-pattern ;

discard-pattern =
    "_" ;

mutable-binding-pattern =
    "mut" identifier ;

literal-pattern =
      integer-literal
    | real-literal
    | imaginary-literal
    | boolean-literal
    | character-literal
    | string-literal ;

nullable-absent-pattern =
    "none" ;

irrefutable-nullable-present-pattern =
    "?" irrefutable-pattern-primary ;

case-nullable-present-pattern =
    "?" case-pattern-primary ;

irrefutable-box-pattern =
    "box" "(" irrefutable-pattern ")" ;

case-box-pattern =
    "box" "(" case-pattern ")" ;

leading-dot-irrefutable-pattern =
    "." identifier [ irrefutable-payload-pattern-body ] ;

leading-dot-case-pattern =
    "." identifier [ case-payload-pattern-body ] ;

path-irrefutable-pattern =
    path [ path-irrefutable-pattern-continuation ] ;

path-case-pattern =
    path [ path-case-pattern-continuation ] ;

path-irrefutable-pattern-continuation =
      irrefutable-payload-pattern-body
    | irrefutable-product-pattern-body ;

path-case-pattern-continuation =
      case-payload-pattern-body
    | case-product-pattern-body ;

expected-type-irrefutable-product-pattern =
    irrefutable-product-pattern-body ;

expected-type-case-product-pattern =
    case-product-pattern-body ;

grouped-irrefutable-pattern =
    "(" irrefutable-pattern ")" ;

grouped-case-pattern =
    "(" case-pattern ")" ;

irrefutable-tuple-pattern =
    "(" irrefutable-pattern "," [ irrefutable-tuple-pattern-tail ] ")" ;

irrefutable-tuple-pattern-tail =
    irrefutable-pattern { "," irrefutable-pattern } [ "," ] ;

case-tuple-pattern =
    "(" case-pattern "," [ case-tuple-pattern-tail ] ")" ;

case-tuple-pattern-tail =
    case-pattern { "," case-pattern } [ "," ] ;

irrefutable-array-pattern =
    "[" irrefutable-array-pattern-entry-sequence [ "," ] "]" ;

irrefutable-array-pattern-entry-sequence =
    irrefutable-array-pattern-entry { "," irrefutable-array-pattern-entry } ;

irrefutable-array-pattern-entry =
      irrefutable-pattern
    | remaining-pattern ;

case-array-pattern =
    "[" case-array-pattern-entry-sequence [ "," ] "]" ;

case-array-pattern-entry-sequence =
    case-array-pattern-entry { "," case-array-pattern-entry } ;

case-array-pattern-entry =
      case-pattern
    | remaining-pattern ;

irrefutable-product-pattern-body =
    "{" [ irrefutable-product-pattern-entry-sequence [ "," ] ] "}" ;

irrefutable-product-pattern-entry-sequence =
    irrefutable-product-pattern-entry { "," irrefutable-product-pattern-entry } ;

irrefutable-product-pattern-entry =
      product-field-irrefutable-pattern
    | remaining-pattern ;

product-field-irrefutable-pattern =
    identifier [ "=" irrefutable-pattern ] ;

case-product-pattern-body =
    "{" [ case-product-pattern-entry-sequence [ "," ] ] "}" ;

case-product-pattern-entry-sequence =
    case-product-pattern-entry { "," case-product-pattern-entry } ;

case-product-pattern-entry =
      product-field-case-pattern
    | remaining-pattern ;

product-field-case-pattern =
    identifier [ "=" case-pattern ] ;

irrefutable-payload-pattern-body =
    "(" [ irrefutable-payload-pattern-entry-sequence [ "," ] ] ")" ;

irrefutable-payload-pattern-entry-sequence =
    irrefutable-payload-pattern-entry { "," irrefutable-payload-pattern-entry } ;

irrefutable-payload-pattern-entry =
      named-irrefutable-payload-pattern
    | irrefutable-pattern
    | remaining-pattern ;

named-irrefutable-payload-pattern =
    identifier "=" irrefutable-pattern ;

case-payload-pattern-body =
    "(" [ case-payload-pattern-entry-sequence [ "," ] ] ")" ;

case-payload-pattern-entry-sequence =
    case-payload-pattern-entry { "," case-payload-pattern-entry } ;

case-payload-pattern-entry =
      named-case-payload-pattern
    | case-pattern
    | remaining-pattern ;

named-case-payload-pattern =
    identifier "=" case-pattern ;

remaining-pattern =
    ".." ;
```

The grammar excludes `|` alternatives from `irrefutable-pattern` positions.

Semantic checking still verifies that an `irrefutable-pattern` matches every value of its subject type.

`irrefutable-recursive-pattern` and `case-recursive-pattern` are the two recursive specializations of the same pattern shape. They
are separate grammar productions because their child pattern roots differ.

A bare `identifier` is parsed through `path-irrefutable-pattern` or `path-case-pattern`. Pattern-context name resolution decides
whether it is a binding pattern or a named pattern.

Product field shorthand is the `identifier` case of a product field pattern.

Payload entries without `=` are checked as positional payload entries while positional payload fields are available. Otherwise they
are checked as payload field shorthand.

`..` is parsed as `remaining-pattern`. Semantic checking enforces where it is allowed and that each pattern body contains at most
one remaining pattern.

`access-expression` is the syntactic subset that can form an access path. It intentionally excludes calls, slice projections,
nullable propagation, conversion, and construction postfixes.

Full struct construction that names a type is the construction-body case of `access-primary-expression` and is named
`full-struct-construction-expression`.

Expected-type struct construction uses `expected-type-struct-construction-expression`.

Leading-dot union variant syntax is a primary root. Payload variant construction then uses the ordinary call postfix.

Comparisons are non-associative because `comparison-expression` accepts at most one comparison operator.

Exponentiation is right-associative and binds tighter than prefix unary operators because the right operand of `**` is a
`unary-expression`.

Restricted roots reuse the ordinary non-assignment expression ladder. This preserves ordinary expression precedence while excluding
assignment at the syntax level. Context-specific validity, such as constant-evaluation validity, predicate-expression validity, and
guard-expression validity, is checked semantically.

```ebnf
constant-expression =
    non-assignment-expression ;

predicate-expression =
    non-assignment-expression ;

guard-expression =
    non-assignment-expression ;
```

```ebnf
comparison-operator =
      "=="
    | "!="
    | "<"
    | "<="
    | ">"
    | ">=" ;

shift-operator =
      "<<"
    | ">>" ;

additive-operator =
      "+"
    | "-" ;

multiplicative-operator =
      "*"
    | "/"
    | "%"
    | "@" ;

unary-operator =
      "-"
    | "~"
    | "!" ;
```

---

## Type expressions

Type expressions use a separate non-left-recursive grammar from runtime expressions.

```ebnf
type-expression =
    prefix-type-expression ;

prefix-type-expression =
      borrow-type-expression
    | box-type-expression
    | view-type-expression
    | callable-type-expression
    | postfix-type-expression ;

borrow-type-expression =
    "&" [ "mut" ] type-expression ;

box-type-expression =
    "box" [ type-form-argument-list ] type-expression ;

view-type-expression =
    "view" trait-application ;

callable-type-expression =
    callable-directives callable-modifiers "func" parameter-list
    [ callable-result-clause ] callable-contract-clauses ;

postfix-type-expression =
    type-primary-expression { type-postfix-operation } ;

type-postfix-operation =
      nullable-type-operation
    | generic-argument-list
    | qualified-type-member-operation ;

nullable-type-operation =
    "?" ;

qualified-type-member-operation =
    "(" trait-application ")" "." identifier ;

type-primary-expression =
      self-type-expression
    | unit-type-expression
    | path
    | grouped-type-expression
    | tuple-type-expression
    | slice-type-expression
    | array-type-expression ;

self-type-expression =
    "Self" ;

unit-type-expression =
    "unit" ;

grouped-type-expression =
    "(" type-expression ")" ;

tuple-type-expression =
    "(" type-expression "," [ tuple-type-tail ] ")" ;

tuple-type-tail =
    type-expression { "," type-expression } [ "," ] ;

slice-type-expression =
    "[" type-expression "]" ;

array-type-expression =
    "[" type-expression ";" array-extent "]" ;

array-extent =
      constant-expression
    | ".." ;

trait-application =
    path [ generic-argument-list ] ;

generic-argument-list =
    "<" generic-argument-sequence [ "," ] ">" ;

generic-argument-sequence =
    generic-argument { "," generic-argument } ;

generic-argument =
      type-expression
    | constant-expression ;

type-form-argument-list =
    "[" type-form-argument-sequence [ "," ] "]" ;

type-form-argument-sequence =
    type-form-argument { "," type-form-argument } ;

type-form-argument =
      type-expression
    | constant-expression ;
```

Prefix type forms consume a complete type expression as their subject. Postfix type operations bind to the nearest primary type
expression. For example, `box[Heap] Point?` is a box whose subject is `Point?`. `(box[Heap] Point)?` is a nullable box.

`Self` is a keyword type expression in trait and implementation contexts.

When a default-storage `box` subject begins with `[`, the subject is grouped so it is not parsed as a type-form argument list:
`box ([u8])`.

`view` uses a trait application as its subject. Trait applications use a path plus optional generic arguments.

Qualified type-valued member references are parsed as postfix operations on a type expression. Subjects that need explicit grouping
use `grouped-type-expression`, as in `(&Vec<T>)(Iterable).Cursor`.

A parenthesized single type expression without a comma is grouping. A one-element tuple type uses the trailing-comma form.

Slice and array type expressions both begin with `[`. A semicolon after the element type selects an array form. A constant extent
creates a fixed-size array. The `..` extent creates an incomplete trailing-array layout form whose valid declaration contexts are
checked semantically.

Generic arguments and type-form arguments can syntactically contain type expressions or constant expressions. The accepted argument
kinds are checked by the selected declaration or type form.

Callable type forms use the shared callable roots without a name or body.

---

## Typed identifiers and type annotations

Several declarations bind a required name to a required type expression.

```ebnf
typed-identifier =
    identifier type-annotation ;

type-annotation =
    ":" type-expression ;
```

The grammar uses `typed-identifier` only where a plain identifier name and a required type annotation are part of the same
declaration item. Patterns that may include an optional type annotation keep their own grammar.

---

## Callable parameters

Callable parameter lists are shared by callable declarations, callable type forms, named callable contracts, lifecycle declarations,
and lambda expressions.

```ebnf
parameter-list =
    "(" [ parameter-sequence [ "," variadic-parameter ] [ "," ] ] ")" ;

variadic-parameter =
    "..." ;

parameter-sequence =
    parameter { "," parameter } ;

parameter =
    parameter-modifiers typed-identifier [ parameter-default ] ;

parameter-modifiers =
    { parameter-modifier } ;

parameter-modifier =
      "pos"
    | "mut" ;

parameter-default =
    "=" expression ;
```

The grammar allows empty parameter lists and a trailing comma after the final parameter. An ellipsis can follow one or more fixed
parameters. Semantic checking restricts variadic forms to supported foreign ABI callable contracts.

Parameter modifiers are written before the parameter name. Duplicate modifiers and context-invalid modifier combinations are
semantic errors.

A `pos` parameter can appear only before parameters that do not have `pos`.

The `mut` modifier marks the parameter binding. Borrowing and reachable mutation are represented by the parameter's type
expression.

Parameter defaults use expression syntax. Default validity, evaluation timing, effects, capabilities, and omission rules are
checked by the call and declaration rules.

---

## Callable contract clauses

Callable contract clauses are shared by callable declarations, callable type forms, named callable contracts, lifecycle
declarations, and lambda expressions.

```ebnf
callable-contract-clauses =
    { callable-contract-clause } ;

callable-contract-clause =
      requires-clause
    | ensures-clause
    | with-clause
    | uses-clause ;

requires-clause =
    "requires" "(" predicate-expression-sequence [ "," ] ")" ;

ensures-clause =
    "ensures" "(" predicate-expression-sequence [ "," ] ")" ;

with-clause =
    "with" "(" static-predicate-expression-sequence [ "," ] ")" ;

uses-clause =
    "uses" "(" trusted-capability-sequence [ "," ] ")" ;

predicate-expression-sequence =
    predicate-expression { "," predicate-expression } ;

static-predicate-expression-sequence =
    static-predicate-expression { "," static-predicate-expression } ;

static-predicate-expression =
      trait-satisfaction-constraint
    | predicate-expression ;

trait-satisfaction-constraint =
    type-expression ":" trait-application ;

trusted-capability-sequence =
    trusted-capability { "," trusted-capability } ;

trusted-capability =
    path ;
```

Each contract clause contains at least one entry and can include a trailing comma.

`requires(...)` and `ensures(...)` contain predicate expressions.

`with(...)` contains predicate expressions checked in static constraint context.

`uses(...)` contains trusted capability paths.

Clause ordering, repeated clause kinds, and context-specific clause availability are checked by the declaration and contract rules.

---

## Constant declarations

An ordinary constant declaration introduces a named compile-time value.

```ebnf
constant-declaration =
    constant-modifiers "const" typed-identifier "=" constant-expression ";" ;

constant-modifiers =
    [ visibility-modifier ] ;
```

The type annotation is required.

The initializer is checked as a constant expression.

Visibility modifiers are valid only in declaration contexts that support declaration visibility.

Constant-valued trait members use the same declaration shape, with their own member-specific initializer rules.

---

## Static declarations

Static declarations introduce address-bearing product or native-thread storage.

```ebnf
static-declaration =
    static-directives static-declaration-modifiers "static" [ "mut" ] identifier
    [ generic-parameter-list ] ":" type-expression { with-clause }
    static-declaration-tail ;

static-directives =
    { static-directive } ;

static-directive =
      thread-local-directive
    | link-directive
    | symbol-directive ;

thread-local-directive =
    directive-marker "thread_local" ;

static-declaration-modifiers =
    { static-declaration-modifier } ;

static-declaration-modifier =
      "extern"
    | "trusted"
    | visibility-modifier ;

static-declaration-tail =
      "=" constant-expression ";"
    | ";" ;
```

`static` declares product storage. `@thread_local` selects storage for each attached native thread. The directive accepts no
arguments and cannot be repeated.

Generic parameters, when present, are written after the declaration name. Header `with(...)` clauses establish static constraints
for the declaration.

The type annotation is required. A Bray-owned static has an initializer checked as a constant expression template. An extern
static ends with `;` because its provider supplies the storage definition and initialization.

Visibility modifiers are valid because static declarations are module-level declarations. `@link(...)` and `@symbol(...)` can
describe a native data symbol. `extern` and `trusted` participate in the foreign-storage contract. `mut` follows `static` and
declares externally mutable storage without granting source mutation authority.

The complete storage, specialization, access, dependency, and cleanup rules are defined in
[Static storage declarations](declarations/static-storage-declarations.md).

---

## Generic parameters

Generic parameter lists are shared by declarations that can introduce type parameters, constant parameters, or both.

```ebnf
generic-parameter-list =
    "<" generic-parameter-sequence [ "," ] ">" ;

generic-parameter-sequence =
    generic-parameter { "," generic-parameter } ;

generic-parameter =
      generic-type-parameter
    | generic-const-parameter ;

generic-type-parameter =
    identifier ;

generic-const-parameter =
    "const" typed-identifier ;
```

A generic parameter list contains at least one parameter and can include a trailing comma.

Bare generic parameter names declare type parameters.

Const parameters use `const NAME: Type`.

---

## Predicate declarations

Predicate declarations introduce contract-level relations.

```ebnf
predicate-declaration =
    predicate-modifiers "predicate" identifier [ generic-parameter-list ]
    predicate-parameter-list predicate-declaration-tail ;

predicate-modifiers =
    { predicate-modifier } ;

predicate-modifier =
      visibility-modifier
    | "trusted" ;

predicate-declaration-tail =
      "=" predicate-expression ";"
    | ";" ;

predicate-parameter-list =
    "(" [ predicate-parameter-sequence [ "," ] ] ")" ;

predicate-parameter-sequence =
    predicate-parameter { "," predicate-parameter } ;

predicate-parameter =
    typed-identifier ;
```

The declaration name follows `predicate`.

Generic parameters, when present, are written after the predicate name and before the parameter list.

Visibility is a declaration header modifier. `public` is optional because it is the default.

The `trusted` modifier declares an opaque trusted predicate.

An ordinary predicate declaration uses an `=` tail with a single predicate expression body.

A trusted predicate declaration uses a semicolon tail and has no ordinary body.

Predicate parameters use names and type annotations. Predicate parameters do not use callable parameter modifiers or defaults.

Predicate modifiers can appear in any source order. Duplicate modifiers and context-invalid modifier combinations are semantic
errors.

---

## Callable contract declarations

Named callable contract declarations introduce reusable names for callable type forms.

```ebnf
callable-contract-declaration =
    callable-contract-modifiers "callable" identifier [ generic-parameter-list ]
    callable-contract-constraints "=" callable-type-expression ";" ;

callable-contract-modifiers =
    [ visibility-modifier ] ;

callable-contract-constraints =
    { with-clause } ;
```

The declaration name follows `callable`.

Generic parameters, when present, are written after the callable contract name.

Header `with(...)` clauses establish static constraints for the named callable contract. The right-hand callable type form can
reference the declaration's generic parameters and constraints.

Visibility is the declaration header modifier. ABI directives, callable modifiers, result clauses, and callable contract clauses
belong to the right-hand callable type form.

The declaration has no body and ends with `;`.

---

## Type declarations

Type declarations introduce named product and union types.

```ebnf
type-declaration =
      struct-declaration
    | union-declaration ;

struct-declaration =
    type-directives type-modifiers "struct" identifier [ generic-parameter-list ]
    type-constraints struct-declaration-tail ;

struct-declaration-tail =
      struct-body
    | ";" ;

union-declaration =
    type-directives type-modifiers "union" identifier [ generic-parameter-list ]
    type-constraints union-body ;

type-directives =
    { type-directive } ;

type-directive =
      layout-directive
    | copy-directive ;

layout-directive =
    directive-marker "layout" directive-argument-list ;

copy-directive =
    directive-marker "copy" ;

type-modifiers =
    [ visibility-modifier ] ;

type-constraints =
    { with-clause } ;
```

The declaration name follows `struct` or `union`.

Generic parameters, when present, are written after the type name.

Header `with(...)` clauses establish static constraints for the type declaration.

Type directives apply to the primary representation declaration. Directive-specific validity is checked semantically.

Visibility is the declaration header modifier. `public` is optional because it is the default.

A semicolon struct tail declares a bodyless type with no forgeable fields. Layout semantics decide whether it is incomplete or has
explicit opaque size and alignment. Union declarations always have bodies.

### Struct bodies

```ebnf
struct-body =
    "{" { struct-body-item } "}" ;

struct-body-item =
      struct-field-declaration
    | type-member-declaration ;

struct-field-declaration =
    field-modifiers typed-identifier [ field-default ] ";" ;

field-modifiers =
    [ visibility-modifier ] [ "mut" ] ;

field-default =
    "=" expression ;
```

Struct fields are semicolon-terminated.

Field visibility and mutability modifiers appear before the field name. The grammar order is visibility, then `mut`.

A field default follows the field type and uses ordinary expression syntax.

Type member declarations are parsed as type-body items and are defined by the grammar for constructors, lifecycle declarations,
and other type-owned declarations.

### Union bodies

```ebnf
union-body =
    "{" { union-body-item } "}" ;

union-body-item =
      union-variant-declaration
    | type-member-declaration ;

union-variant-declaration =
    variant-directives identifier [ union-variant-payload ] ";" ;

variant-directives =
    { variant-directive } ;

variant-directive =
    tag-directive ;

tag-directive =
    directive-marker "tag" directive-argument-list ;

union-variant-payload =
    "(" [ union-payload-field-sequence [ "," ] ] ")" ;

union-payload-field-sequence =
    union-payload-field { "," union-payload-field } ;

union-payload-field =
    payload-field-modifiers typed-identifier [ field-default ] ;

payload-field-modifiers =
    [ "pos" ] [ "mut" ] ;
```

Union variants are semicolon-terminated.

A no-payload variant omits the payload parentheses.

Payload fields are comma-separated inside variant parentheses and can include a trailing comma.

Payload field modifiers appear before the payload field name. The grammar order is `pos`, then `mut`.

Variant-level visibility modifiers are not part of union bodies. A variant has the visibility surface of its union type.

---

## Type member declarations

Type member declarations are definitions written inside a `struct` or `union` body.

```ebnf
type-callable-member-declaration =
    type-callable-member-modifiers "func" identifier [ generic-parameter-list ]
    parameter-list [ callable-result-clause ] callable-contract-clauses
    callable-body-block-expression ;

type-callable-member-modifiers =
    { type-callable-member-modifier } ;

type-callable-member-modifier =
      visibility-modifier
    | callable-modifier
    | "static"
    | "consume"
    | "mut" ;
```

The function name follows `func`.

Generic parameters, when present, are written after the function name and before the parameter list.

A type callable member is either a static function or an instance method.

`static` selects a static function with no receiver.

Without `static`, the member is an instance method. The receiver mode is selected by the receiver modifiers:

- no receiver modifier selects a shared receiver,
- `mut` selects a mutable receiver,
- `consume` selects a consuming receiver,
- `consume mut` selects a consuming receiver whose method body has mutable local authority over `self`.

Visibility, callable modifiers, receiver modifiers, and `static` are parsed as modifiers before `func`. Duplicate modifiers,
incompatible modifier combinations, and invalid receiver-mode combinations are semantic errors.

Each type callable member is a definition and has a callable body block.

### Type Constants

A constant declaration inside a type body defines a type-associated constant.

Type-associated constants use ordinary constant declaration syntax.

### Type Predicates

A predicate declaration inside a type body defines a type-associated predicate.

Type-associated predicates use ordinary predicate declaration syntax.

### Constructors

```ebnf
type-constructor-member-declaration =
    constructor-member-modifiers "construct" [ identifier ] parameter-list
    callable-result-clause callable-contract-clauses callable-body-block-expression ;

constructor-member-modifiers =
    { constructor-member-modifier } ;

constructor-member-modifier =
      visibility-modifier
    | "trusted" ;
```

`construct` without a following identifier declares the primary constructor.

`construct` followed by an identifier declares a named constructor under the type.

The result clause is required. Semantic checking accepts `Self` or `Result<Self, E>` according to lifecycle rules.

Constructors are synchronous. Trusted constructors use the same `trusted` modifier and callable contract clauses as other trusted
declarations.

### Lifecycle Members

```ebnf
type-lifecycle-member-declaration =
      finalizer-member-declaration
    | destructor-member-declaration
    | scope-enter-member-declaration
    | scope-exit-member-declaration ;

finalizer-member-declaration =
    async-capable-lifecycle-member-modifiers "finalize" empty-parameter-list
    [ callable-result-clause ] callable-contract-clauses callable-body-block-expression ;

destructor-member-declaration =
    sync-lifecycle-member-modifiers "destruct" empty-parameter-list
    [ callable-result-clause ] callable-contract-clauses callable-body-block-expression ;

scope-enter-member-declaration =
    scope-enter-member-modifiers "enter" empty-parameter-list
    callable-result-clause callable-contract-clauses callable-body-block-expression ;

scope-exit-member-declaration =
    async-capable-lifecycle-member-modifiers "exit" single-parameter-list
    [ callable-result-clause ] callable-contract-clauses callable-body-block-expression ;

async-capable-lifecycle-member-modifiers =
    { async-capable-lifecycle-member-modifier } ;

async-capable-lifecycle-member-modifier =
      "async"
    | "trusted" ;

scope-enter-member-modifiers =
    { scope-enter-member-modifier } ;

scope-enter-member-modifier =
      "async"
    | "trusted"
    | "consume"
    | "mut" ;

sync-lifecycle-member-modifiers =
    { sync-lifecycle-member-modifier } ;

sync-lifecycle-member-modifier =
    "trusted" ;

empty-parameter-list =
    "(" ")" ;

single-parameter-list =
    "(" parameter [ "," ] ")" ;
```

Finalizers, destructors, scope enter declarations, and scope exit declarations use lifecycle keywords instead of user-chosen
function names.

Finalizers, destructors, and scope exit declarations can omit the result clause. Omitted result means `unit`.

Scope enter declarations require a result clause because the result type names the scoped capability value made available to the
`with` body.

Scope exit declarations take exactly one scoped-capability parameter.

Constructors have no receiver and constructor bodies have no `self` binding.

Finalizers have an implicit mutable receiver. Destructors have an implicit consuming mutable receiver.

Scope enter declarations use the ordinary receiver modes. No receiver modifier selects a shared receiver, `mut` selects a mutable
receiver, `consume` selects a consuming receiver, and `consume mut` selects a consuming receiver with mutable local authority.

Scope exit declarations have no receiver. They operate on their scoped-capability parameter and can reach the original value only
through access carried by that capability.

Each lifecycle member is a definition and has a callable body block.

---

## Trait declarations

Trait declarations introduce named behavioral contracts.

```ebnf
trait-declaration =
    trait-modifiers "trait" identifier [ generic-parameter-list ]
    trait-constraints trait-body ;

trait-modifiers =
    [ visibility-modifier ] ;

trait-constraints =
    { with-clause } ;

trait-body =
    "{" { trait-member-declaration } "}" ;
```

The declaration name follows `trait`.

Generic parameters, when present, are written after the trait name.

Header `with(...)` clauses establish static constraints for the trait declaration.

Visibility is the declaration header modifier. `public` is optional because it is the default.

Trait members inherit the visibility of the trait. Individual trait members do not accept `public` or `internal`.

### Trait Callable Members

```ebnf
trait-callable-member-declaration =
    trait-callable-member-modifiers "func" identifier [ generic-parameter-list ]
    parameter-list [ callable-result-clause ] callable-contract-clauses
    trait-callable-member-tail ;

trait-callable-member-modifiers =
    { trait-callable-member-modifier } ;

trait-callable-member-modifier =
      callable-modifier
    | "static"
    | "consume"
    | "mut" ;

trait-callable-member-tail =
      callable-body-block-expression
    | ";" ;
```

A semicolon tail declares a required callable member.

A body tail declares default behavior for implementations that omit the member.

Without `static`, the member is an instance method. The receiver mode is selected by the receiver modifiers:

- no receiver modifier selects a shared receiver,
- `mut` selects a mutable receiver,
- `consume` selects a consuming receiver,
- `consume mut` selects a consuming receiver whose method body has mutable local authority over `self`.

`static` selects a static trait function with no receiver.

Callable modifiers, receiver modifiers, and `static` are parsed as modifiers before `func`. Duplicate modifiers, incompatible
modifier combinations, and invalid receiver-mode combinations are semantic errors.

### Trait Constant Members

```ebnf
trait-constant-member-declaration =
    "const" typed-identifier [ "=" constant-expression ] ";" ;
```

A trait constant member without an initializer is required.

A trait constant member with an initializer supplies a default value.

### Trait Type Members

```ebnf
trait-type-member-declaration =
    "type" identifier ";" ;
```

A trait type member declares a required type-valued member.

Trait type-valued members cannot have generic parameters or defaults.

### Trait Predicate Members

```ebnf
trait-predicate-member-declaration =
    trait-predicate-member-modifiers "predicate" identifier
    predicate-parameter-list trait-predicate-member-tail ;

trait-predicate-member-modifiers =
    [ "trusted" ] ;

trait-predicate-member-tail =
      "=" predicate-expression ";"
    | ";" ;
```

A semicolon tail declares a required predicate member.

An `=` tail supplies a predicate body.

Trait predicate members use the shared `predicate-parameter-list` grammar.

### Trait Lifecycle Requirements

```ebnf
trait-lifecycle-requirement-declaration =
      trait-finalizer-requirement-declaration
    | trait-destructor-requirement-declaration
    | trait-scope-enter-requirement-declaration
    | trait-scope-exit-requirement-declaration ;

trait-finalizer-requirement-declaration =
    async-capable-lifecycle-member-modifiers "finalize" empty-parameter-list
    [ callable-result-clause ] callable-contract-clauses ";" ;

trait-destructor-requirement-declaration =
    sync-lifecycle-member-modifiers "destruct" empty-parameter-list
    [ callable-result-clause ] callable-contract-clauses ";" ;

trait-scope-enter-requirement-declaration =
    scope-enter-member-modifiers "enter" empty-parameter-list
    callable-result-clause callable-contract-clauses ";" ;

trait-scope-exit-requirement-declaration =
    async-capable-lifecycle-member-modifiers "exit" single-parameter-list
    [ callable-result-clause ] callable-contract-clauses ";" ;
```

Trait lifecycle requirements use lifecycle keywords and end with semicolons.

Lifecycle requirements do not have bodies in trait declarations.

Constructor requirements are expressed as static callable members that return `Self` or `Result<Self, E>`.

---

## Implementation declarations

Implementation declarations introduce inherent behavior or fulfill a trait application for an implementing subject.

```ebnf
implementation-declaration =
      inherent-implementation-declaration
    | unnamed-trait-implementation-declaration
    | named-trait-implementation-declaration ;

inherent-implementation-declaration =
    "impl" inherent-implementation-subject implementation-constraints
    implementation-body ;

unnamed-trait-implementation-declaration =
    "impl" implementation-subject "(" trait-application ")"
    implementation-constraints implementation-body ;

named-trait-implementation-declaration =
    "impl" identifier "=" implementation-subject "(" trait-application ")"
    implementation-constraints implementation-body ;

implementation-constraints =
    { with-clause } ;
```

An inherent implementation has only a subject.

An unnamed trait implementation has a subject followed by a trait application in parentheses.

A named trait implementation introduces the implementation identity before `=`.

Implementation declarations do not have explicit generic parameter lists. Generic implementation parameters are inferred from
otherwise unresolved generic names in the implementing subject and, for trait implementations, the trait application.

Header `with(...)` clauses establish static constraints for the implementation. A `with(...)` clause can constrain inferred
implementation parameters, but it cannot introduce them.

### Implementation Subjects

```ebnf
inherent-implementation-subject =
    implementation-named-subject ;

implementation-subject =
    [ implementation-borrow-prefix ] implementation-named-subject ;

implementation-borrow-prefix =
    "&" [ "mut" ] ;

implementation-named-subject =
    path [ generic-argument-list ] ;
```

An inherent implementation subject is a named type subject.

A trait implementation subject can be a named subject, a generic named subject, an inferred implementation parameter subject, or
an implementation-eligible borrow subject.

The grammar accepts `&Subject` and `&mut Subject` as borrow-subject forms. Which type forms are implementation-eligible is checked
by type rules.

### Implementation Bodies

```ebnf
implementation-body =
    "{" { implementation-member-declaration } "}" ;

implementation-member-declaration =
      type-callable-member-declaration
    | type-constructor-member-declaration
    | type-lifecycle-member-declaration
    | implementation-type-member-binding
    | constant-declaration
    | predicate-declaration
    | callable-overload-declaration ;

implementation-type-member-binding =
    "type" identifier "=" type-expression ";" ;
```

Implementation bodies are braced member lists.

The parser uses the same member grammar for inherent and trait implementation bodies.

### Implementation Members

Implementation members reuse the same definition forms as type-body callable members, constructors, lifecycle members,
type-associated constants, type-valued member bindings, type-associated predicates, and callable overload declarations.

An inherent implementation type member binds a name to a type expression associated with the implementation subject.

The binding does not introduce a module-level type alias and does not fulfill a trait requirement.

An inherent implementation predicate declaration defines a type-associated predicate for the implementation subject.

Trait implementation callable members always have a body.

Trait implementation constant members always have an initializer.

Trait implementation type members are bindings from a required trait type-valued member to a concrete type expression.

Trait implementation predicate members always have a predicate body.

Trait implementation lifecycle members can define `enter` and `exit` fulfillments.

Individual trait implementation members cannot use `public` or `internal` modifiers. They are fulfillments of the implemented
trait contract.

Missing members, extra members, duplicate members, signature compatibility, type-valued member compatibility, lifecycle
fulfillment, and coherence are semantic checks.

---

## Overload declarations

Overload declarations introduce explicit overload families.

```ebnf
overload-declaration =
      callable-overload-declaration
    | implementation-overload-declaration ;
```

Module-level overload declarations can be callable overload declarations or implementation overload declarations.

Type bodies and implementation bodies can contain callable overload declarations.

Trait declarations do not contain overload declarations.

Callable overload declarations in trait implementation bodies are rejected semantically because they do not fulfill trait members.

### Callable Overload Declarations

```ebnf
callable-overload-declaration =
    overload-modifiers "overload" identifier "=" overload-arm-list ;
```

The identifier after `overload` is the shared callable surface.

Visibility is the declaration header modifier. `public` is optional because it is the default.

Callable overload arms name existing callable declarations.

A callable overload declaration does not define a callable body.

### Implementation Overload Declarations

```ebnf
implementation-overload-declaration =
    overload-modifiers "overload" implementation-overload-subject "(" path ")" "="
    overload-arm-list ;

overload-modifiers =
    [ visibility-modifier ] ;

implementation-overload-subject =
      implementation-overload-named-subject
    | grouped-implementation-overload-borrow-subject ;

implementation-overload-named-subject =
    path ;

grouped-implementation-overload-borrow-subject =
    "(" implementation-borrow-prefix implementation-overload-named-subject ")" ;
```

Overload modifiers are shared by callable and implementation overload declarations.

An implementation overload declaration groups named trait implementations under a shared subject and trait surface.

The subject before the trait parentheses names the shared implementing subject declaration.

Borrow subjects use the grouped forms `(&Subject)` and `(&mut Subject)`.

The path inside the trait parentheses names the shared trait declaration, not a concrete trait application.

### Overload Arms

```ebnf
overload-arm-list =
    "{" overload-arm-sequence [ "," ] "}" ;

overload-arm-sequence =
    overload-arm { "," overload-arm } ;

overload-arm =
    path ;
```

An overload arm list contains at least one arm and can include a trailing comma.

Callable overload arms must resolve to callable declarations valid for the overload context.

Implementation overload arms must resolve to named trait implementation declarations valid for the implementation overload
family.

Duplicate arms, incompatible arms, visibility, coherence, overlap, and overload selection are semantic checks.

---

## Callable and function directives

Callable directives are part of the callable contract and can also appear on callable type forms and ABI-qualified lambdas.

```ebnf
callable-directives =
    { callable-directive } ;

callable-directive =
    abi-directive ;

abi-directive =
    directive-marker "abi" directive-argument-list ;
```

Function directives apply to module-level function declarations.

```ebnf
function-directives =
    { function-directive } ;

function-directive =
      callable-directive
    | link-directive
    | symbol-directive
    | entrypoint-directive
    | test-directive ;

link-directive =
    directive-marker "link" directive-argument-list ;

symbol-directive =
    directive-marker "symbol" directive-argument-list ;

entrypoint-directive =
    directive-marker "entrypoint" ;

directive-argument-list =
    "(" [ directive-argument-sequence [ "," ] ] ")" ;

directive-argument-sequence =
    directive-argument { "," directive-argument } ;

directive-argument =
    [ identifier "=" ] constant-expression ;
```

Directive arguments use constant-expression syntax.

Directive-specific argument names, required arguments, allowed positional entries, duplicate directives, and declaration contexts
are semantic checks.

---

## Function declarations

This production defines module-level function declarations.

```ebnf
function-declaration =
    function-directives function-modifiers "func" identifier
    [ generic-parameter-list ] parameter-list [ callable-result-clause ]
    callable-contract-clauses function-declaration-tail ;

function-modifiers =
    { function-modifier } ;

function-modifier =
      "extern"
    | visibility-modifier
    | callable-modifier ;

function-declaration-tail =
      callable-body-block-expression
    | ";" ;
```

The function name follows `func`.

Generic parameters, when present, are written after the function name and before the parameter list.

The result clause is optional. An omitted result type means `unit`.

The grammar accepts either a callable-body block or `;`. Extern functions use `;`. Non-extern functions use a callable-body block.
Context-specific body requirements are semantic checks.

Function modifiers can appear in any source order. Duplicate modifiers and incompatible combinations are semantic errors.

Trait members, implementation members, lifecycle declarations, and callable type forms reuse the shared callable roots but define
their own declaration productions.

---

## Lambda expressions

Lambda expressions use the shared callable grammar roots for directives, parameters, result types, contract clauses, and callable
body blocks.

```ebnf
lambda-expression =
    callable-directives callable-modifiers "lambda" parameter-list
    [ callable-result-clause ] callable-contract-clauses callable-body-block-expression ;

callable-modifiers =
    { callable-modifier } ;

callable-modifier =
      "async"
    | "trusted"
    | "const" ;

callable-result-clause =
    "->" type-expression ;

callable-body-block-expression =
    block-expression ;
```

`parameter-list` includes the surrounding parentheses.

`callable-directive`, `parameter-list`, `callable-result-clause`, `callable-contract-clause`, and
`callable-body-block-expression` are shared callable grammar roots. Function declarations, lambda expressions, callable contracts,
and callable type forms use those same roots.

The callable modifier repetition accepts any source order. Duplicate modifiers and incompatible combinations are semantic errors.

When a callable modifier sequence is followed by `lambda`, the parser treats it as a `lambda-expression`. This makes
`trusted lambda ...` a trusted lambda expression, not a trust-boundary expression whose operand is a lambda.

---

## Lexical terminals

These terminals are defined by the lexical grammar:

```ebnf
tuple-element-index =
    (* defined in lexical-grammar.ebnf *) ;

integer-literal =
    (* defined in lexical-grammar.ebnf *) ;

real-literal =
    (* defined in lexical-grammar.ebnf *) ;

imaginary-literal =
    (* defined in lexical-grammar.ebnf *) ;

character-literal =
    (* defined in lexical-grammar.ebnf *) ;

string-literal =
    (* defined in lexical-grammar.ebnf *) ;

identifier =
    (* defined in lexical-grammar.ebnf *) ;
```
