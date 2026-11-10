# Bray for Rust developers, part 2: Model data and control flow

In [part 1](01-build-and-organize.md), we built a pantry inventory and learned enough Bray to make the donkey count oats. Now we'll give it a little more responsibility and see how it handles missing oats, invalid quantities, and the occasional existential crisis.

We'll build a small executable that classifies stock, looks up quantities, handles recoverable errors, and catches a panic. Along the way, we'll compare Bray's unions and patterns with Rust's enums and `match`, and look at what `try`, `?`, and `catch` actually do. The symbols should look familiar, but some of them have been reassigned.

This part assumes you're familiar with Rust, including enums, pattern matching, `Option`, and `Result`. It also assumes the Bray basics from part 1, such as named arguments, borrows, and `bray run`. You won't need the previous project.

You'll need an installed Bray toolchain with `bray` on your path and support for your native target. Create an executable project and enter its directory:

```sh
bray init pantry_flow --package pantry_flow
cd pantry_flow
```

## Give stock a state

In Rust, we could use an enum to represent the states of an inventory item:

```rust
enum Stock {
    Empty,
    Low(i32),
    Ready(i32),
}
```

In Bray, we use a `union` for this. It has a closed set of variants, with exactly one active variant at a time. It's a tagged sum type, not the overlapping-storage `union` you may know from Rust.

Replace `src/main.bray` with:

```bray
module app;

using std.io;

union Stock
{
    Empty;
    Low(pos quantity: i32);
    Ready(pos quantity: i32);
}

func describe(pos status: &Stock) -> string
{
    return match status
    {
        case Stock.Empty { yield "Empty"; }
        case Stock.Low(_) { yield "Low stock"; }
        case Stock.Ready(_) { yield "Stock is ready"; }
    };
}

func main()
{
    let status: Stock = Stock.Low(2);

    std.io.print_line(describe(&status));
}
```

Run it:

```sh
bray run
```

The application prints:

```text
Low stock
```

`Empty` has no payload. `Low` and `Ready` each carry a quantity. Just like function parameters in part 1, `pos` allows us to supply that quantity positionally. That's why `Stock.Low(2)` works. Without `pos`, we'd have to also supply the payload's name. Nothing particularly strange so far. But the expressions in the `match` arms contain a keyword that Rust developers might find a little unfamiliar.

### Blocks and `yield`

In Rust, a block can produce its final expression as a value:

```rust
let threshold = {
    let base = 2;
    base + 1
};
```

Bray supports value-producing blocks too, but uses an explicit `yield` instead of relying on the final expression:

```bray
let threshold: i32 =
{
    let base: i32 = 2;

    yield base + 1;
};
```

The block supplies `3` to `threshold`. Bray doesn't treat the final expression as the block's value. You have to say what you intend to yield. An additional keyword, yes, but at least a stray semicolon can't change the meaning of your block. A block in a `unit` context can finish without a `yield` and produces `unit`. If a block must produce a non-`unit` value, however, every normal completion path must yield one. Otherwise, the donkey brays and the compiler rejects it.

> **IMPORTANT!** `yield` supplies a value to the nearest enclosing yield-capable region. It does **not** return from the function. That distinction matters, as we'll see shortly. Bray also uses `yield` in generators, but we'll leave those for another time.

### Match every variant

Rust's `match` can select a branch and produce a value. Bray's `match` does the same, but every arm begins with `case`. Each arm body is a block expression, so its `yield` provides that arm's value. Take another look at `describe`:

```bray
return match status
{
    case Stock.Empty { yield "Empty"; }
    case Stock.Low(_) { yield "Low stock"; }
    case Stock.Ready(_) { yield "Stock is ready"; }
};
```

The selected arm yields a string to the `match` expression, and the outer `return` exits `describe` with that string. The `yield` itself doesn't exit the function. Notice that we've handled all three variants: `Empty`, `Low`, and `Ready`. Remove any one of them and Bray rejects the match as non-exhaustive, just as Rust would. The `_` in `Low(_)` and `Ready(_)` ignores the quantity. We only care about the variant here, not its payload. Also, because `status` is a shared borrow, `describe` can inspect the stock without taking ownership. The oats remain safely in the caller's possession, which is reassuring given how much attention we're paying them.

## Test a pattern without a match block

Rust provides the `matches!` macro when you want to check whether a value has a particular shape. Bray doesn't have macros, so it provides a `matches` expression instead.

Add this to `main`, after the call to `describe`:

```bray
if status matches Low(_)
{
    std.io.print_line("Time to reorder");
}
```

Run `bray run` again. The application now prints:

```text
Low stock
Time to reorder
```

The `matches` expression observes `status` and returns a `bool`. It evaluates the subject once and doesn't consume it. However, `matches` is strictly a yes-or-no question. Its pattern cannot introduce bindings, so this is valid:

```bray
status matches Low(_)
```

But this isn't:

```bray
status matches Low(quantity)
```

If we want to actually use the quantity, we'll need something else. Fortunately, Bray didn't forget about that.

## Bind a matching value

Rust's `if let` lets us extract a payload when a pattern matches. Bray's conditional `let` does the same, using Bray's pattern syntax.

Add this after the `matches` condition:

```bray
if let Low(remaining) = status && remaining == 2
{
    std.io.print_line("Two left");
}
```

Run `bray run`. The output gains another line:

```text
Two left
```

The `remaining` binding is available both to the later `&&` condition and inside the body. The condition short-circuits, meaning `remaining == 2` is only evaluated after the `Low` pattern has successfully matched. No need to nest another `if` just to check the value we extracted (Bray may be opinionated, but it isn't *that* cruel). And because the earlier `matches` expression didn't consume `status`, we can inspect it again here. We're still only *observing* the value, though.

## Propagate an absent value

In Rust, a function returning `Option<T>` can use `?` to return `None` early. Bray provides the same kind of shortcut, but represents nullable types as `T?` and absence as `none`. We already encountered these in part 1, so this should look familiar.

Add these functions to `src/main.bray`:

```bray
func find_quantity(name: string) -> i32?
{
    if name == "oats"
    {
        return 2;
    }

    return none;
}

func reorder_quantity(name: string) -> i32?
{
    let quantity: i32 = find_quantity(name = name)?;

    if quantity < 3
    {
        return 3 - quantity;
    }

    return 0;
}
```

Add these statements to `main`:

```bray
if let ?needed = reorder_quantity(name = "oats") && needed == 1
{
    std.io.print_line("Order one");
}

if reorder_quantity(name = "rice").is_absent()
{
    std.io.print_line("Rice not found");
}
```

Run `bray run`. The new lines are:

```text
Order one
Rice not found
```

For oats, `find_quantity` returns `2`, and `reorder_quantity` calculates how many more we need to reach three.

For rice, `find_quantity` returns `none`. The postfix `?` immediately propagates that absence out of `reorder_quantity`, before any subtraction happens. No need to manually check for `none` and return it again. We've already told the compiler what to do.

The second condition only needs to know whether the result is absent, so it uses `is_absent()` without binding a value. And as in part 1, `if let ?needed` binds the present value. It doesn't propagate absence.

Yes, that's the same question mark doing another job, this time on the pattern side of `=`. Deal with it.

## Return a recoverable error

Sometimes an item is missing. Other times, somebody tells us we have negative stock. In Rust, `Result<T, E>` distinguishes a successful value from a recoverable error. Bray also has `Result<T, E>`, with `Ok` and `Error` variants. Unlike Rust's `Err`, Bray is bold enough to not abbreviate absolute everything (just like `func` vs `fn`).

Okay, let's reject negative quantities. Add these declarations to `src/main.bray`:

```bray
union StockError
{
    NegativeQuantity;
}

func classify(quantity: i32) -> Result<Stock, StockError>
{
    if quantity < 0
    {
        return Error(NegativeQuantity);
    }

    if quantity == 0
    {
        return Ok(Stock.Empty);
    }

    if quantity < 3
    {
        return Ok(Stock.Low(quantity));
    }

    return Ok(Stock.Ready(quantity));
}

func load_status(quantity: i32) -> Result<Stock, StockError>
{
    let status: Stock = try classify(quantity = quantity);

    return Ok(status);
}
```

Add this to `main`:

```bray
let invalid: Result<Stock, StockError> = load_status(quantity = -1);

if invalid matches Error(_)
{
    std.io.print_line("Quantity cannot be negative");
}
```

Run `bray run`. The application adds:

```text
Quantity cannot be negative
```

Good. Our inventory system now has enough common sense. The `classify` function returns an ordinary `Result` value. In `load_status`, the prefix `try` unwraps an `Ok` value or propagates an `Error`. Since we passed `-1`, `classify` returns `Error(NegativeQuantity)`. That error propagates through `load_status` and reaches `main`, where `matches Error(_)` checks its shape.

This is entirely normal, recoverable control flow. No panic, no drama. Just an invalid quantity being treated as an invalid quantity.

### `try` versus `?`

For this example:

```bray
let status: Stock = try classify(quantity = quantity);
```

plays the same role as Rust's:

```rust
let status = classify(quantity)?;
```

Remember that Bray's postfix `?` propagates nullable absence, not `Result` errors. For those, we use `try`.

Error conversion is also a little different. Rust uses the general `From` conversion mechanism when propagating compatible errors. Bray instead follows its built-in [result propagation rules](../../language/expressions/result-and-run-result-propagation-expressions.md).

Here, both functions use the same `StockError`, so no conversion is needed. The distinction is intentional. Bray prefers explicit rules for error propagation over an arbitrary conversion mechanism.

## Move a payload out of a union

So far, we've only inspected union values. But sometimes we actually need to take ownership of a variant's payload. In Rust, matching an owned enum can move a non-`Copy` payload out of it. In Bray, an ordinary `match` observes its subject. If you want to move a non-copyable payload out, you need `match consume`, which consumes the original union value.

Add these declarations to `src/main.bray`:

```bray
struct Item
{
    name: string;
}

union Delivery
{
    Arrived(pos item: Item);
    Missing;
}

func take_item(pos delivery: Delivery) -> Item?
{
    return match consume delivery
    {
        case Arrived(item) { yield item; }
        case Missing { yield none; }
    };
}
```

Add this to `main`:

```bray
let item: Item =
{
    name = "oats",
};

let delivery: Delivery = Delivery.Arrived(item);

if let ?delivered = take_item(delivery)
{
    std.io.print_line(delivered.name);
}
```

Run `bray run`. The application adds:

```text
oats
```

The `Item` type has no copy contract, so we can't simply duplicate it. To return the payload from the `Arrived` arm, we have to move it. That's what `match consume delivery` permits. The `Arrived` arm moves the item out and yields it. The `Missing` arm yields `none`.

An ordinary observing `match` wouldn't permit moving out the non-copyable payload. The call to `take_item` also transfers ownership of `delivery`, so we can't use that value afterward. We've handed it over. It's gone. The compiler won't entertain arguments about how we only wanted to borrow it for a moment. The earlier `matches`, conditional `let`, and ordinary `match` examples observe their subjects. They're suitable when we only need to inspect a value. `match consume` is for when we actually want to take the oats home.

## Catch a panic

Rust distinguishes a `Result::Err` from a panic, and so does Bray. A `Result.Error` is an ordinary value that callers can inspect or propagate with `try`. A panic, on the other hand, interrupts normal control flow until it's caught or the run ends. Bray provides a `catch` expression that evaluates an operand and returns a `Result` containing either the value or a `PanicReport`.

Let's see how that works, starting with something that **doesn't** panic.

### Catching something that isn't a panic

First, let's prove that `catch` doesn't turn an ordinary error into a panic. Add this to `main`:

```bray
let caught_error: Result<Result<Stock, StockError>, PanicReport> = catch load_status(quantity = -1);

if caught_error matches Ok(Error(_))
{
    std.io.print_line("The error remains a value");
}
```

Run `bray run`. The application adds:

```text
The error remains a value
```

The call completed normally by returning `Error(NegativeQuantity)`. Since no panic occurred, `catch` wraps the returned value in an outer `Ok`. That means our result has this shape:

```bray
Ok(Error(NegativeQuantity))
```

The outer `Ok` means the operation didn't panic. The inner `Error` means the operation returned a recoverable error. Two different things, represented separately. And yes, this gives us a nested `Result`. It's perfectly valid (although if you start nesting another four layers, you might want to reconsider your life choices).

### Catch an actual panic

Now let's make something go properly wrong. Add this function to `src/main.bray`:

```bray
func count_missing_oats()
{
    panic("The oats have escaped");
}
```

It's a drastic response to missing oats, but perhaps the situation is worse than we thought. Add this to `main`:

```bray
let caught: Result<unit, PanicReport> = catch count_missing_oats();

if caught matches Error(_)
{
    std.io.print_line("Recovered from a panic");
}
```

Run `bray run`. The application adds:

```text
Recovered from a panic
```

The function panics while `catch` evaluates its operand. Instead of allowing the panic to terminate the run, `catch` converts it into an `Error(PanicReport)` value. The panicked function doesn't resume. We're recovering control at the catch boundary. This is broadly similar to Rust's `std::panic::catch_unwind`, except Bray's `catch` is a language expression and produces Bray's `PanicReport`. Also, `catch` doesn't turn every ordinary `Error` into a panic, nor does it catch task cancellation. Those are different mechanisms with different meanings. We'll look at task outcomes a later part.

For now, our application can distinguish an invalid quantity from a genuine panic.

## Check the finished program

We've added quite a few things to our small application. Let's make sure it still behaves as expected. Run it once more:

```sh
bray run
```

Apart from any build progress, the complete output should be:

```text
Low stock
Time to reorder
Two left
Order one
Rice not found
Quantity cannot be negative
oats
The error remains a value
Recovered from a panic
```

Our pantry now has distinct ways to represent stock states, absence, recoverable errors, and panics. For an optional exercise, change:

```bray
load_status(quantity = -1)
```

to:

```bray
load_status(quantity = 0)
```

in the `invalid` binding. Running the application again, the `Quantity cannot be negative` line should disappear, because the result is now `Ok(Stock.Empty)` rather than an error. Leave the separate `caught_error` call unchanged, so its line still appears. Then restore `-1` afterward.

## Next up

We've covered tagged unions, exhaustive pattern matching, conditional bindings, nullable propagation, recoverable errors, ownership transfer through `match consume`, and panic recovery (no more panic attacks).

Part 3 will take a closer look at values, storage, ownership, and copying. If you're coming from Rust, some of it will feel familiar, while other parts will give the donkey another opportunity to disagree with you.

The language reference has the full rules for [unions](../../language/types/union-types.md), [patterns](../../language/patterns.md), [nullable propagation](../../language/expressions/nullable-and-absence-expressions.md), [result propagation](../../language/expressions/result-and-run-result-propagation-expressions.md), and [catch expressions](../../language/expressions/catch-expressions.md).