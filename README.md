# OrimaLang

A plain-English programming language implemented in Rust.

OrimaLang lets you write programs using natural English sentences.
The only symbols allowed are commas `,` (clause separator), periods `.` (statement terminator), and double-quotes `"..."` (string literals).
Keywords are case-insensitive.

---

## Quick Example

```ori
note This is a comment.
set name to nothing.

if name is nothing,
  ask "what is your name" and store in name,
end if.

say "hello" and name.

set score to 0.
repeat 5 times, increase score by 10, end repeat.
say "final score is" and score.

if score is at least 40, say "you win", otherwise say "try harder", end if.

create list fruits.
add "apple" to fruits.
add "banana" to fruits.
for each fruit in fruits, say fruit, end for.

define greet taking person, say "hello" and person, end define.
run greet with "World".

define type Dog taking name and breed,
  define bark, say "Woof I am" and name of self, end define,
end type.

create Dog with "Rex" and "Labrador" and store in myDog.
run bark on myDog.
```

---

## Language Reference

### Comments

```ori
note This line is ignored.
```

### Values and Types

Type is inferred from the value:

```ori
set name to "Ori".       note string
set age to 25.           note number
set flag to true.        note boolean (true / false)
set result to nothing.   note no value (like null)
```

### Variables

```ori
set age to 25.
set name to "Ori".
set flag to true.
set x to the value of y.
```

### Nothing (null)

`nothing` represents the absence of a value. It is falsy in conditions.

```ori
set name to nothing.

if name is nothing,
  ask "enter your name" and store in name,
end if.

say name.
```

### Arithmetic Statements

```ori
increase score by 10.
decrease lives by 1.
multiply price by 2.
divide total by 4.
```

### Math Expressions

```ori
set total to price plus tax.
set diff to big minus small.
set area to width times height.
set half to total divided by 2.
set rem to number remainder of divided by 3.
```

### Output

```ori
say "hello".
say "your name is" and name and "nice to meet you".
```

`and` in a `say` statement concatenates values with spaces.

### Input

```ori
ask "enter your name" and store in name.
```

The prompt is shown to the user. The variable after `store in` receives the typed value.

### Conditions

```ori
if age is greater than 17, say "adult", end if.
if name is "Ori", say "hello Ori", otherwise say "hello stranger", end if.
```

**Comparison operators:**

| Phrase | Meaning |
| --- | --- |
| `is` | equality |
| `is not` | inequality |
| `is greater than` | `>` |
| `is less than` | `<` |
| `is at least` | `>=` |
| `is at most` | `<=` |
| `contains` | substring check |
| `starts with` | prefix check |
| `ends with` | suffix check |

**Logical operators:**

```ori
if age is greater than 17 and also flag is true, say "ok", end if.
if x is 1 or y is 2, say "yes", end if.
if it is not the case that flag is true, say "nope", end if.
```

### Loops

```ori
repeat 5 times, say "hello", end repeat.

set count to 1.
while count is at most 10, say count, increase count by 1, end while.

for each fruit in fruits, say fruit, end for.
```

**Loop control:**

```ori
stop the loop.
skip to next.
```

### Lists (1-indexed)

```ori
create list fruits.
add "apple" to fruits.
add "banana" to fruits.
remove "apple" from fruits.
remove item 2 from fruits.
say item 1 from fruits.
say the size of fruits.
```

### Functions

```ori
define greet taking name, say "hello" and name, end define.
run greet with "Ori".

define add numbers taking a and b, give back a plus b, end define.
run add numbers with 10 and 20 and store in result.
say result.
```

Functions have their own scope. Parameters are passed by value. Use `give back` to return a value.

### Types (Objects)

**Define a type** with fields and optional methods:

```ori
define type Dog taking name and breed,
  define bark,
    say "Woof I am" and name of self,
  end define,
  define rename taking new_name,
    set name of self to new_name,
  end define,
end type.
```

**Create an instance:**

```ori
create Dog with "Rex" and "Labrador" and store in myDog.
```

**Read a field:**

```ori
say name of myDog.
set x to breed of myDog.
```

**Set a field:**

```ori
set name of myDog to "Max".
```

**Call a method:**

```ori
run bark on myDog.
run rename on myDog with "Buddy".
run add on myObj with 10 and 20 and store in result.
```

Inside a method, use `field of self` to read fields and `set field of self to value` to write them. Changes to `self` persist on the object after the method returns.

### Text Operations

```ori
set full to first joined with last.
set len to the length of name.
set upper to name in uppercase.
set lower to name in lowercase.
```

---

## Building

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable, edition 2021)

### 1. Native CLI

**Install globally** (adds `orima` to `~/.cargo/bin`):

```bash
cargo install --path .
```

Or build without installing:

```bash
cargo build --release
./target/release/orima app.ori
```

**Run a file:**

```bash
orima app.ori
```

**Interactive REPL:**

```bash
orima repl
```

The REPL buffers input until a period `.` is seen, so multi-line statements work naturally.
Type `quit.` to exit.

---

### 2. WASM Web Playground

The WASM build does not use `wasm-pack`'s built-in cargo invocation (it uses an unstable flag). Instead, build manually:

```bash
# One-time setup
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

# Build the WASM binary
cargo build --target wasm32-unknown-unknown --release --features wasm

# Generate JS bindings
wasm-bindgen \
  --target web \
  --out-dir www/pkg \
  target/wasm32-unknown-unknown/release/orimalang.wasm
```

**Serve the `www/` directory** with any static HTTP server:

```bash
python3 -m http.server 8080 --directory www
```

Then open `http://localhost:8080` in your browser (note: `http://`, not `https://`).

---

## Project Structure

```text
OrimaLang/
├── Cargo.toml
├── src/
│   ├── main.rs        CLI entry point (orima <file.ori> / orima repl)
│   ├── lib.rs         Public API + WASM exports
│   ├── lexer.rs       Tokenizer (handles "..." string literals)
│   ├── parser.rs      Recursive-descent parser → AST
│   └── evaluator.rs   Tree-walk interpreter + environment
└── www/
    ├── index.html     Web playground UI
    └── index.js       WASM loader + UI wiring
```

---

## Error Messages

All errors include a description and line number:

```text
OrimaLang Error: undefined variable 'foo' on line 3
OrimaLang Error: unknown statement keyword 'xyz' on line 7
OrimaLang Error: index 5 out of range for list 'fruits' on line 12
```
