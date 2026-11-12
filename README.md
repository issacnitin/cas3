# CAS3: can a cellular automaton count out the primes?

<p align="center">
  <img src="docs/demo.svg" width="624" alt="Animation of the automaton: a triangle of cells, one per number, fills up with gold so that the number of lit cells is always the next prime, while the cells above it work out whether they are prime">
</p>

A **cellular automaton** is a grid of cells that all follow the same rule. At every step, each
cell looks at its immediate neighbours and decides what to become next. The best-known example is
Conway's Game of Life.

This project asks a simple question:

> Is there a rule, in some number of dimensions, so that if you start with one lit cell and keep
> applying the rule, the number of lit cells is always the next prime?

```
step        0   1   2   3   4   5    6    7    8    9    10   ...
lit cells   1   2   3   5   7   11   13   17   19   23   29   ...
```

## The short answer

- **No simple rule does it.** We searched every rule in many families of simple rules (the
  Game of Life is one of them), from every possible starting pattern, on square, hexagonal and
  cubic grids and in up to 21 dimensions. The best simple rule follows the primes as far as 2, 3,
  5, 7, 11, 13, 17 and then gives 21. Rules that match longer stretches only manage it by
  memorising the list.
- **A rule that does it exists, and we built one.** It is the animation above, and it works in two
  dimensions. Its cells do more than switch on and off: each one quietly works out whether its
  number is prime, and exactly when its turn comes to light up.
- **A three-dimensional version stays exact forever**, with only a limited amount of information
  in each cell. A two-dimensional version could most likely do the same, but hasn't been built.
- **One dimension is impossible**, for any rule at all: a line of cells can't grow fast enough.

## Demo

The animation at the top is drawn straight from the real rule. In it:

- every square is one number: 1 is at the bottom left, the next row holds 2 and 3, the next 4, 5
  and 6, and so on;
- **gold** cells are lit, and they are the ones being counted. After step n, the cells from 1 up to
  the n-th prime are lit;
- **white** cells lit up in this very step: each step lights the stretch up to the next prime;
- **blue** cells already know the exact step at which they will light up;
- the cells above the "end of the table" mark work this out for themselves. **Dark grey** means
  still testing divisors, then comes **grey** (not prime) or **orange** (prime), then blue. The
  rows below the mark take their timing from a small table built into the rule (see below).

You can also watch it in a terminal:

```
cargo run --release -- demo
```

It shows the first 16 rows up close, with their numbers, then zooms out to 64 rows to show the
cells at work. `--speed 2` plays it twice as fast and `--plain` prints the frames as text. Here
are two steps of the plain output (lit cells are in brackets):

```
    11   12   13   14   15
  [  7]   8    9   10
  [  4][  5][  6]
  [  2][  3]
  [  1]

  step 4: 7 cells are lit, and 7 is the 4th prime
```

```
    16   17   18   19   20   21
  [ 11]  12   13   14   15
  [  7][  8][  9][ 10]
  [  4][  5][  6]
  [  2][  3]
  [  1]

  step 5: 11 cells are lit, and 11 is the 5th prime
```

To check many more steps than the animation shows, `cargo run --release -- primeca --steps 20000`
runs the automaton for 20,000 steps (up to the prime 224,737) and checks the count after each one.

## How the automaton works

**One cell per number.** The cells form a staircase: row 1 holds the number 1, row 2 holds 2 and
3, row 3 holds 4, 5 and 6, and so on. Each row is one cell longer than the row below and carries
on counting where that row stopped. New rows are added at the top as the automaton runs, a little
ahead of when they are needed.

**Each cell lights up at the right moment, and stays lit.** The cell for the number n lights up at
step "1 + the number of primes smaller than n". Take cells 8, 9, 10 and 11: each has four smaller
primes (2, 3, 5 and 7), so all four light up at step 5. After step 5 the lit cells are exactly 1 to
11, which makes 11 cells, and 11 is the 5th prime. The same happens at every step: after step n,
the lit cells are 1 up to the n-th prime.

**So every cell has to find out how many primes are smaller than its number, before its moment
comes.** A cell can only talk to its eight neighbours, so this happens in stages:

1. **Am I prime?** The cell tries dividing its number by 2, 3, 4, … one divisor per step, until
   one divides it or the divisor passes the square root.
2. **Count along the row.** Starting from the right-hand end of each row, every cell tells its left
   neighbour how many primes there are from there to the end of the row.
3. **Running total up the left edge.** The first cell of each row hears from the row below how many
   primes there are in all the rows below. It adds its own row's primes and passes the total up.
4. **Back along the row.** The total is passed to the right, cell by cell. Each cell subtracts the
   primes from itself to the end of its row, which leaves the number of primes smaller than itself.
5. **Wait.** Every cell carries a clock, and lights up when the clock reaches its moment.

**Why it is never late.** This work takes about √n steps (testing divisors) plus a few times the
row length (passing counts along). The moment of cell n comes after about n / ln n steps, which
grows much faster. So high enough up, every cell is ready long before its moment: in the
animation, the blue always stays ahead of the gold. Near the bottom there isn't enough time, so the
rule contains a small table with the moments of the first 50 rows (the numbers 1 to 1,275). With a
40-row table the cells would be only just on time, and with 30 rows they would be late.

**What a cell holds.** Besides its visible on/off switch, a cell carries a few hidden whole
numbers: its own number, its row, the clock, the divisor it is trying and its counts. Only the
switch is counted. In cellular-automaton terms, each cell has a very large but finite number of
possible states, where the Game of Life has two (on and off).

**Lasting forever: the 3D version.** The 2D version stores each hidden number as a 64-bit integer,
which eventually overflows: it is exact for every step before step 114,179,426,570,746,131 (about
10¹⁷). A true cellular automaton must have a fixed, finite set of possible states, so its cells
can't hold numbers that grow without limit. The 3D version fixes this. Once numbers get too big for
one cell, each number becomes a stack of digits rising into the third dimension, one 64-bit digit
per cell. The stack grows taller as the numbers grow, so there is no limit. Every stage above is
then done digit by digit, like adding and dividing by hand, with carries passed up the stack. Until
stacks are needed (around step 1.5 × 10¹⁶), the 3D version is exactly the 2D version.

## Why no simple rule works

Before building anything, we searched. The project's original search wrote rules as logical
formulas and tried them one by one; it was so slow that it was only practical in one dimension. The
new search engine is many orders of magnitude faster because it:

- only fills in the parts of a rule that the simulation actually uses, so one failed run rules out
  a whole family of rules at once;
- drops a rule as soon as a step gives the wrong count;
- treats rules that are rotations or mirror images of each other as one;
- covers every possible starting pattern by checking just a handful of two-cell patterns, with a
  proof that nothing else needs checking.

What it found:

- **Simple, symmetric rules fail fast.** From any starting pattern, Life-like rules on a square
  grid get only as far as 2, 3, 5. The best simple family is on a hexagonal grid, and it gets 2, 3,
  5, 7, 11, 13, 17 and then 21.
- **More dimensions make it worse.** In more dimensions a symmetric rule ties more cells together,
  so the count jumps by too much at once. For two large families this is proven for every
  dimension; for example, no Life-like rule in 3 or more dimensions can even produce 2, 3, 5.
  Symmetric families were checked up to 21 dimensions, and other families up to 10.
- **The cheapest near-misses are fooled by 25.** The simplest rules that get furthest actually
  produce the numbers that aren't divisible by 2 or 3: 5, 7, 11, 13, 17, 19, 23, and then
  25 = 5 × 5.
- **Longer matches are memorisation.** With big enough rule tables, a search matched 72 primes in a
  row. But each extra prime cost about three more table entries: the rule was storing the list,
  not producing it. For simple sequences such as 1, 2, 3, 4, … or the square numbers, the number of
  entries needed stops growing straight away. For the primes it never did.

The lesson: a rule that keeps producing primes has to compute them, and a rule that computes is not
simple. So we built one.

**What was known before.** Cellular automata can already signal the primes in time. A known
one-dimensional automaton (Fischer, 1965; Korec, 1997) switches one particular cell on exactly at
the prime-numbered steps, and the Game of Life has a pattern that does something similar
(Hickerson's "Primer", 1991). We know of no earlier work on the version asked here, where the
number of lit cells must be the primes.

## Does the number of dimensions mean anything?

It is tempting to read something deep into the dimension, for instance about how many dimensions
the universe has. What this project shows is more modest:

- **1D is ruled out for a simple reason.** A line of cells can only grow by a fixed number of cells
  per step, but the primes eventually outgrow that (the n-th prime is roughly n × ln n).
- **2D is enough.** A flat sheet has plenty of room, and the rule above works there.
- **3D was used for convenience**, to give each big number room to stack its digits. It could most
  likely be flattened back into 2D.

So the dimension tells you how much room a computation needs, not something about prime numbers or
about physics.

## How we know it works

- **Two separate simulators** run the 2D rule. One updates every cell at every step; the other
  updates only the cells that can change. They agree cell by cell, and the count equals the n-th
  prime after every one of the first 20,000 steps, up to the prime 224,737.
- **A timing certificate** shows that every cell is ready before its moment, for every row the
  64-bit version can hold. It uses a proven lower bound on how many primes there are (Dusart,
  2010). The closest call is row 52, with 26 steps to spare (31 in the actual simulation).
- **The 3D version** was checked in four ways:
  - It was run with tiny digits (3 to 16 bits) so that stacks appear after a few rows, and counted
    correctly at every step up to the prime 12,853. Tiny digits make the work slow, so these runs
    start the clock late.
  - Its arithmetic was tested with full-size 64-bit digits, including where stacks grow a new
    digit.
  - Its countdowns were tested with the real clock.
  - A timing certificate covers every row, forever: numerically up to row 10¹⁵⁰, and by an
    estimate beyond. In the worst case the cells need at most 27% of the time they have.
- **The tests can fail.** Bugs planted on purpose in the countdown logic are all caught. Run
  `cargo test --release` for the whole suite.

## What this is not

- **It is not a simple rule.** It is closer to a small program running in every cell. The searches
  strongly suggest that no simple rule exists, though that can't be proven for every rule.
- **Cells hold hidden numbers.** Only the on/off switch is counted, but cells carry extra
  information. A version where every cell is purely on or off, like the Game of Life, was started
  (see "Constructive route" in the [technical notes](docs/technical-notes.md)) but isn't finished.
- **The first 1,275 numbers come from a table.** The table is a fixed, finite part of the rule, but
  it is a lookup, not a computation.
- **The 3D version hasn't been run at full scale.** Its stacks only start after about 10¹⁶ steps.
  Its correctness there rests on testing each mechanism with full-size 64-bit digits and on the
  timing certificate.

## Run it yourself

You need Rust (`cargo`).

| command | what it does |
|---|---|
| `cargo run --release -- demo` | watch the automaton in the terminal |
| `cargo run --release -- demo --svg docs/demo.svg` | redraw the animation at the top of this page |
| `cargo run --release -- primeca --steps 20000` | run the 2D automaton and check the count after every step |
| `cargo run --release -- primeca --certificate` | timing certificate for the 2D version |
| `cargo run --release -- primecols --certificate` | timing certificate for the 3D version |
| `cargo run --release -- primecols --w 4 --k 6000 --rows 40` | run the 3D version with 4-bit digits, so stacks appear from row 2 |
| `cargo run --release -- search --class totalistic --any-start --terms 30` | try every Life-like rule from every starting pattern |
| `cargo run --release -- sweep --min-dim 2 --max-dim 21 --terms 14` | symmetric rule families in 2 to 21 dimensions |
| `cargo run --release -- help` | every command and option |
| `cargo test --release` | the test suite |

## What's in the repository

| file | what it holds |
|---|---|
| `src/prime_display.rs` | the 2D automaton, its two simulators and its timing certificate |
| `src/prime_columns.rs` | the 3D version, with numbers as stacks of digits |
| `src/demo.rs` | the terminal demo and the animation |
| `src/table_search.rs`, `src/rule_class.rs`, `src/dimension_sweep.rs`, `src/lattice.rs` | the searches for a simple rule |
| `src/bounds.rs` | the impossibility proofs (one dimension, symmetric rules) |
| `src/construct.rs`, `src/prime_worker.rs`, `src/scheduler.rs` and others | the unfinished on/off-only construction |
| `src/state_explorer.rs`, `src/space.rs`, `src/rule.rs` | the original formula-based search (`legacy`) |
| `docs/technical-notes.md` | the detailed record: every search result, proof sketch and design decision |

## Where this started

Prime numbers are intriguing: every other number is built from them. This project began as a
search over rules written as formulas, dimension by dimension, hoping to find a pattern whose cell
count is the primes. That first search found rules for sequences such as 1, 2, 3, 4, …, the odd
numbers and the squares, and once 1, 3, 5, 9, 11, 15, 19, 27, but never the primes. The rest of
the story is above.

![An early run of the original search](./example.gif)

Contributions are welcome. If you can make the search faster, or find a simpler rule, open a pull
request.
