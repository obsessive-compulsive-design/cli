# Strict JS: Language Specification

Oct 25, 2026 · @Someone

## 1. Overview

Strict JS is a pure subset of ECMAScript 2025: every Strict JS program is an ordinary `.js` module that runs unchanged in any ES2025 engine and any ordinary test runner, and the compiler adds nothing to it. The compiler is a whole-program checker that infers every type, rejects the constructs listed in this spec, reports style deviations, and runs tests.

**Goals**

- Zero syntax additions. No annotations, no new keywords, no new file extension. If a file passes the compiler it was already valid JavaScript before the compiler saw it.
- Full static type safety by inference alone. The program is a closed world (only its own files and the runtime's globals), so the checker sees every call site and every construction site and can infer unions, generics, and discriminants without a single annotation.
- Non-duplicative: where ECMAScript offers two spellings of one idea, exactly one survives.
- Portable by construction: the only ambient API is the set of globals present in both browsers and server runtimes. Nothing platform-specific is reachable from the language.
- JIT-friendly: fixed object shapes, no holes, no prototype mutation, no `arguments`/`eval`.

**Non-goals**

- Importing anything that is not a file in the project. There are no packages, no bare specifiers, no declaration files.
- Accessing the DOM, the filesystem, the process, or any API that exists on only one side. Code that needs those lives outside the subset and calls into it.
- Emitting different code. The compiler never rewrites a program; an `|0`, a polyfill or a reordered import would change what runs, and the programmer must always be reading the code that runs.

**Design principles**

1. Subtract only. The language is defined by what it removes from ES2025. The output of `sjs build` is the input, byte for byte, plus diagnostics.
2. One nullish value you create (`undefined`), one equality (`===`), one declaration form per intent (`const`, `let`, `function`, arrow, `class`).
3. Everything is inferred. Public function signatures come from bodies and call sites; unions come from the set of values that reach a binding; exhaustiveness is checked against inferred unions.
4. Correctness findings are errors that fail the build. Style findings are reported with the exact change the formatter would make and are applied only when the programmer asks (`sjs format --write`), so no tool ever changes a file silently.
5. Tests are plain JavaScript. A test file uses the `describe`/`it`/`expect` globals that Jest, Vitest, Mocha+Chai and the Strict JS runner all provide, so the same file runs under any of them.

## 2. Lexical structure and grammar

The grammar is ES2025's module grammar with productions deleted and nothing added. Source is UTF-8 in a `.js` file, always strict mode, always a module.

**Lexical rules**

- Statements end at a newline or `;`. Automatic semicolon insertion is never relied on: a line starting with `(`, `[`, `` ` ``, `+`, `-`, `/` after an expression line, or a line terminator between `return`/`throw`/`yield` and its operand, is `E001 ambiguous-line-start`, so the file parses the same way with or without ASI. A \`;\` preceding the line does not exempt it: semicolons are pure style (S002) and the programmer restructures. E001 is a hand edit; the formatter proposes no replacement for it.
- The file begins with `import`, a declaration, a comment or a statement. A `#!` hashbang line is `E000 syntax`: it is a server-only signal and the subset admits nothing that only one side reads.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`. `$` and Unicode escapes are not permitted in identifiers.
- Removed words (using one is `E000 syntax`): `var`, `with`, `void`, `delete`, `in`, `arguments`, `eval`, `debugger`. `null` is permitted only as the right operand of `===`/`!==` (§3.1).
- Every construct the grammar does not accept, and no named code covers, is `E000 syntax`; the message names the removed production and its §12 replacement. ES2025 early errors (duplicate parameters, duplicate `let`/`const` in a scope, `yield` outside a generator, `await` outside `async`, `break` outside a loop) are also `E000`.
- Numeric literals: decimal, `0x`, `0o`, `0b`, exponent, `_` separators, `n` suffix for bigint. No leading-zero octal.
- String literals: `"` only (`'` is reported as style S001). Template literals with `` ` ``. Tagged templates are permitted only with `String.raw`.
- Regex literals must state their flags explicitly, and the flag set must include `v`. A literal with no flags, or with `u` instead of `v`, is `E002 regex-flags`. Flags that affect matching (`g`, `i`, `m`, `s`, `y`, `d`) are written whenever the program depends on them; the checker rejects `matchAll` and `replaceAll` with a regex lacking `g`, and `lastIndex` reads on a regex lacking `g` or `y`.
- Comments: `//` and `/* */`. A `/** */` block directly above an exported declaration, with no blank line between, is its doc comment.

**Grammar (EBNF, deltas only; unlisted productions are ES2025 verbatim)**

```
Module        = { ImportDecl } { ModuleItem } ;
ModuleItem    = ExportDecl | Declaration | Statement ;
ImportDecl    = "import" ( "{" ImportList "}" | "*" "as" Ident ) "from" RelPath
              | "import" Ident "from" JsonPath "with" "{" "type" ":" "\"json\"" "}" ;   (* the only default import *)
JsonPath      = StringLit ;                    (* "./" or "../" path ending in ".json" *)
ImportList    = Ident [ "as" Ident ] { "," Ident [ "as" Ident ] } [ "," ] ;
RelPath       = StringLit ;                    (* must start with "./" or "../" and end in ".js" or ".json" *)
ExportDecl    = "export" ( Declaration | "{" ExportList "}" [ "from" RelPath ] ) ;

Declaration   = ConstDecl | LetDecl | FunctionDecl | ClassDecl ;
ConstDecl     = "const" BindingPattern "=" Expr ;
LetDecl       = "let" BindingPattern "=" Expr ;             (* initializer required *)
FunctionDecl  = [ "async" ] "function" [ "*" ] Ident "(" Params ")" Block ;
ClassDecl     = "class" Ident [ "extends" ( Ident | Ident "." Ident ) ] "{" { ClassMember } "}" ;
ClassMember   = FieldDecl | MethodDecl | AccessorDecl | "static" Block ;
FieldDecl     = [ "static" ] ( Ident | PrivName ) [ "=" Expr ] ;
MethodDecl    = [ "static" ] [ "async" ] [ "*" ] MemberKey "(" Params ")" Block ;
AccessorDecl  = [ "static" ] ( "get" MemberKey "(" ")" Block | "set" MemberKey "(" Param ")" Block ) ;
MemberKey     = Ident | PrivName | "[" "Symbol" "." ( "iterator" | "asyncIterator" | "dispose" | "asyncDispose" ) "]" ;

Statement     = Block | IfStmt | ForOfStmt | ForStmt | WhileStmt | SwitchStmt | TryStmt
              | "return" [ Expr ] | "break" | "continue" | "throw" Expr | ExprStmt ;
Block         = "{" { Declaration | Statement } "}" ;      (* every statement body is a Block; braceless bodies are E000 *)
IfStmt        = "if" "(" Expr ")" Block [ "else" ( Block | IfStmt ) ] ;
ForOfStmt     = "for" [ "await" ] "(" ( "const" | "let" ) BindingPattern "of" Expr ")" Block ;
ForStmt       = "for" "(" LetDecl ";" Expr ";" ForUpdate ")" Block ;
ForUpdate     = Ident ( "+=" | "-=" ) Expr | Ident ( "++" | "--" ) | ( "++" | "--" ) Ident ;
WhileStmt     = "while" "(" Expr ")" Block ;
SwitchStmt    = "switch" "(" Expr ")" "{" { CaseClause } [ DefaultClause ] "}" ;
CaseClause    = { "case" Literal ":" } Block ;            (* stacked labels allowed; the Block must end abruptly or be last *)
TryStmt       = "try" Block [ "catch" [ "(" Ident ")" ] Block ] [ "finally" Block ] ;

Expr          = AssignExpr ;
AssignExpr    = CondExpr | LeftHandSide ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "??=" ) AssignExpr ;
CondExpr      = NullishExpr [ "?" AssignExpr ":" AssignExpr ] ;
NullishExpr   = OrExpr [ "??" OrExpr ] ;                   (* ?? may not mix with || or && unparenthesised *)
EqualityExpr  = RelExpr { ( "===" | "!==" ) RelExpr } ;
RelExpr       = AddExpr { ( "<" | ">" | "<=" | ">=" | "instanceof" ) AddExpr } ;
UnaryExpr     = ( "!" | "-" | "+" | "~" | "typeof" | "await" ) UnaryExpr | PostfixExpr ;
PostfixExpr   = PrimaryExpr { "." Ident | "?." Ident | "[" Expr "]" | "?.[" Expr "]" | Call | "?." Call } ;
PrimaryExpr   = Literal | Ident | "this" | ArrayLit | ObjectLit | ArrowFn | TemplateLit
              | "new" ( Ident | Ident "." Ident ) Call | "import" "(" RelPath ")" | "(" Expr ")" ;
ArrowFn       = [ "async" ] "(" Params ")" "=>" ( Expr | Block ) ;    (* parentheses always *)
ObjectLit     = "{" { ( Ident [ ":" Expr ] | StringLit ":" Expr | "[" Expr "]" ":" Expr | "..." Expr ) "," } "}" ;
```

**Removed productions.** `VariableStatement` (`var`), `WithStatement`, `LabelledStatement`, `DoWhileStatement`, `ForInStatement`, `DebuggerStatement`, `Expression` (comma), `FunctionExpression`, `GeneratorExpression`, `ClassExpression`, `DeleteExpression`, `VoidExpression`, `InExpression`, `EqualityExpression` with `==`/`!=`, `LogicalAssignment` with `||=`/`&&=`, `OptionalChain` on `super`, accessors and methods on object literals, `ImportMeta`, `ExportDefault`, `DecoratorList`, `Elision` in array literals, `__proto__` property keys, `NewTarget`, `SuperProperty` outside methods, bare and absolute import specifiers, `LetDecl` without an initializer, `ArrowParameters` without parentheses.

## 3. Values and the inferred type system

Every type is inferred from the program itself; there is no annotation syntax, so the checker's job is to compute, for each expression, the set of values that can reach it and to reject operations that are not defined for every member of that set.

### 3.1 Runtime value kinds

| Kind                    | Inferred type                               | `typeof`                | Notes                                                  |
| ----------------------- | ------------------------------------------- | ----------------------- | ------------------------------------------------------ |
| IEEE-754 double         | `number`                                    | `"number"`              | `NaN` tested only with `Number.isNaN`                  |
| Integer in \[-2³¹, 2³¹) | `int32` (refinement of `number`)            | `"number"`              | inferred, never written; see 3.5                       |
| Arbitrary integer       | `bigint`                                    | `"bigint"`              | never mixes with `number`                              |
| UTF-16 string           | `string`                                    | `"string"`              | iteration yields code points                           |
| Boolean                 | `boolean`                                   | `"boolean"`             |                                                        |
| Absent                  | `undefined`                                 | `"undefined"`           | the only nullish value a program may create            |
| Host null               | `null`                                      | `"object"`              | enters only from `JSON.parse` and host APIs; see below |
| Object                  | object, array, tuple, class, function types | `"object"`/`"function"` | shape fixed at creation                                |

A program may not write the literal `null` except as the right operand of `===` or `!==`. `JSON.parse` and host APIs can still return `null`, so their results are typed to include it and must be narrowed (`x === null`) before use. A function that would return `null` to its own callers is `E100 null-produced`; return `undefined`.

### 3.2 Inference

The checker runs whole-program bidirectional inference over the closed module graph:

- A binding's type is the union of every value assigned to it. A `const` has exactly one.
- A function's parameter types are the union of the argument types at every call site in the program, generalized to a type parameter when the body treats the value opaquely (passes it through, stores it, compares it with `===`, uses it as a `Map`/`Set` key or value). A function that is exported but never called is inferred from its body alone with each parameter generalized.
- A function's return type is the union of its `return` expressions, or `undefined`.
- An object literal's type is its exact shape. A class instance's type is its class (nominal). An array literal's type is the union of its element types.
- An empty `[]`, `new Map()` or `new Set()` takes its element types from the `push`/`unshift`/index-assignment, `set`, or `add` calls within the binding's scope: the function body for a local, the class body for a field, the module for a top-level `const`. With no such call it is `E110 empty-untyped`.
- A property read `o.k` requires every member of `o`'s type to have `k`; a key present on only some members is `E137 missing-property` unless narrowed. A property write requires the key to exist and the value to match; new keys cannot be added after creation.
- Literal types are kept for `const` bindings, object properties set from literals, and `switch`/`===` discriminants, and widened to the primitive elsewhere.
- Generics are let-polymorphic: a function or class is generalized at its declaration and instantiated at each use.

Inference is deterministic and order-independent: the result does not depend on which file is checked first.

### 3.3 Assignability

Structural subtyping for objects, arrays, tuples and functions; nominal for classes; width subtyping for objects except that an object literal passed directly to a parameter or stored in a typed slot may not carry keys the receiver never reads (`E101 excess-property`, a typo detector). Function parameters are contravariant, including method parameters. Mutable arrays are invariant in their element type; arrays that are never mutated anywhere in the program are treated as covariant. `int32 <: number`; every literal type is a subtype of its primitive.

### 3.4 Mutability

There is no annotation, so mutability is inferred from use. A binding, property or array is _mutated_ if the program assigns to it, calls a mutating method on it (`push`, `splice`, `sort`, …) or passes it to a function that does. A value is _frozen_ if it passed through `Object.freeze`, and writing to a frozen value is `E122 write-to-frozen` (it throws in strict mode anyway). The checker reports a `const` object that is never mutated and never frozen as style S020 so the author can decide whether to freeze it; it does not add the freeze.

### 3.5 Narrowing and exhaustiveness

- `typeof x === "string"` and friends; `x === undefined`; `x === null`; `x === <literal>`.
- Discriminant properties: `x.kind === "circle"` when every member of the inferred union has a literal-typed `kind`.
- `x instanceof C` for classes; `Array.isArray(x)`; `Number.isNaN(x)`.
- Predicate functions: a function whose body is a single `return` of a narrowing expression is itself a narrowing when called in a condition.
- Truthiness narrows `T | undefined` to `T` only when `T` has no falsy member; otherwise `E102 ambiguous-truthiness`.
- Narrowing persists along the control-flow path, is invalidated by assignment to a `let`, and is kept across calls for `const` bindings and properties of unmutated objects.

A `switch` on a union of literals must cover every member or have a `default`; a missing member is `E103 non-exhaustive`. An `if/else` chain whose branches all `return` is checked the same way. `int32` is inferred for integer literals in range, array indices, `.length`, `Math.trunc`/`floor`/`ceil`/`round` results, and bitwise results; bitwise operators are permitted only on `int32` or `bigint` operands (`E104 bitwise-on-double`).

### 3.6 Errors

`throw` accepts only `Error` instances. `catch (e)` binds `e` as `unknown`; it must be narrowed with `instanceof` before any property is read. A thrown type is not part of a function's type; code that wants callers to handle failure returns an object with a literal discriminant (`{ ok: true, value }` / `{ ok: false, error }`), which exhaustiveness then enforces.

## 4. Semantics of expressions, statements, functions and classes

Every construct keeps its ES2025 runtime meaning; the checker restricts which inferred operand types may reach it, so no coercion path is ever executed.

### 4.1 Operators

| Operator         | Permitted operand types                                                             | Result                                                 |
| ---------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `+`              | `number×number`, `bigint×bigint`, `string×string`                                   | same kind                                              |
| `- * / % **`     | `number×number`, `bigint×bigint`                                                    | `number` or `bigint` (`%` on `int32×int32` is `int32`) |
| unary `-` `+`    | `number`, `bigint`                                                                  | same                                                   |
| `& \| ^ ~ << >>` | `int32×int32`, `bigint×bigint`                                                      | `int32` or `bigint`                                    |
| `>>>`            | `int32×int32` only; `bigint` is E138 (throws at runtime)                            | `number` (up to 2³²−1)                                 |
| `< > <= >=`      | both `number`, both `bigint`, or both `string`                                      | `boolean`                                              |
| `=== !==`        | any two types that overlap; disjoint types are `E105 no-overlap`                    | `boolean`                                              |
| `! && \|\|`      | `boolean` only                                                                      | `boolean`                                              |
| `??`             | left side must include `undefined` or `null`                                        | union minus nullish                                    |
| `?:`             | condition `boolean`                                                                 | union of branches                                      |
| `typeof`         | any                                                                                 | literal union of the strings in §3.1                   |
| `instanceof`     | right side a class                                                                  | `boolean`, narrows                                     |
| `await`          | a `Promise` only, inside `async`; a non-promise operand is `E119 await-non-promise` | the resolved type                                      |

`&&` and `||` are boolean operators only; a non-boolean operand is an error, so they never return operands. Use `??` for defaults and `?:` for selection. Mixing `??` with `&&`/`||` without parentheses is an error. There is no implicit string coercion: `${x}` in a template requires `x` to be `string`, `number`, `bigint` or `boolean`; objects need an explicit `String(x)` or a `toString()` the checker can see returns `string`.

### 4.2 Bindings

`const` is the default. `let` requires an initializer and is permitted only if the binding is reassigned somewhere in its scope (`E106 unused-let`). Shadowing an outer binding is permitted only in a nested function, not in a nested block (`E107 block-shadow`).

### 4.3 Statements

- Every statement body is a block in braces. `if (x) y()` and `for (…) y()` are `E000`; there is one way to write a body.
- `if` conditions must be `boolean`. `if (x)` on a non-boolean is an error; write `x !== undefined` or `x.length > 0`.
- `for (const x of iterable)` iterates anything with `[Symbol.iterator]`; `for await` anything with `[Symbol.asyncIterator]`. Index loops use `for (let i = 0; i < n; i += 1)`; the header's update is `i += k`, `i -= k`, or `i++`/`--i` and friends. `i++` as a statement elsewhere is style S012; `i++` used as a value is `E000`, because the rewrite to `i += 1` would change the result.
- `while (cond)` with a `boolean` condition; while (true) is the one infinite-loop idiom and its body must contain break, return or throw (E121 otherwise). There is no `do…while`.
- `switch` compares with `===`. Each case body is a block that must `return`, `throw`, `break` or `continue`, or be the last clause; falling through into another case's body is `E108 fallthrough`. Several `case` labels may stack on one block. The scrutinee must be an inferred union of literals, `boolean`, or a discriminant property; switching on an open `string` or `number` requires a `default`.
- `try/catch/finally` as ES2025; `catch {` without a binding is permitted. A `finally` that `return`s is `E109 finally-return`.
- `throw` must throw an `Error` instance.
- `return` with a value in a function whose other paths return nothing, or vice versa, is `E111 mixed-return`. The inferred return type is then unambiguous.

### 4.4 Functions

- `function` declarations are hoisted and must be at module or block top level. They are the form for named, recursive, generator or exported functions.
- Arrow functions are expressions and the form for callbacks and inline values; parameters are always parenthesised. There is no `function` expression.
- Parameter and return types are inferred (§3.2). A parameter that is never constrained by the body and never called with anything is `E112 unconstrained-parameter` on an unexported function; on an exported function it is generalized.
- No `arguments`, no `this` in functions or arrows outside class bodies, no `.call`/`.apply`/`.bind`.
- Generators `function*` and `async function*` are permitted; `yield` only inside them. The generator's element type is the union of its `yield` expressions.
- `async` functions return a promise. A promise that is neither awaited, returned, nor passed to a `Promise.*` combinator is `E113 floating-promise`.
- Default parameters are evaluated per call; a default may not reference `this`.

### 4.5 Classes

- Every instance field is declared in the class body (with or without an initializer) and every field without an initializer is assigned in the constructor on all paths (`E114 uninitialized-field`). Assigning to an undeclared property anywhere is an error, so an instance's shape is its class.
- `#private` fields and methods are the only privacy mechanism.
- `extends` one class, named by identifier. `super()` is called in a derived constructor before `this` is used.
- Getters and setters are permitted on classes only; a setter must accept the getter's type.
- `static` fields, methods and `static {}` blocks are permitted.
- Class expressions are removed; a class is always a declaration, so its name is its type.
- `instanceof` is the nominal test. `constructor`, `prototype`, `Object.getPrototypeOf` and `Object.setPrototypeOf` are not accessible.
- Abstract or interface-like behavior is expressed as a class whose constructor throws, or as a structural shape that several classes happen to satisfy; the checker treats structural compatibility as sufficient wherever an instance is used by its properties.

### 4.6 Objects and arrays

- Object literals produce their exact shape. Spread merges structurally, later keys win. The same key spelled twice is `E134 duplicate-key`. A computed key in a literal must be an inferred literal that is a known key of the resulting shape; any other computed key is `E000`.
- A computed read `o[k]` is permitted when `k` is an inferred literal or a union of literals every member of which is a key of `o`'s shape; the result is the union of those keys' value types. `Object.keys(o)` and `Object.entries(o)` on a fixed shape yield that union, so `for (const k of Object.keys(o)) { o[k] }` types precisely. Any other computed read is `E137 missing-property`.
- Arrays are dense. Indexing an array with an index the checker cannot prove in range yields `T | undefined`, which must be narrowed; a loop variable bounded by `.length`, an index dominated by `i >= 0 && i < arr.length`, or a literal index into a tuple is proven. Tuples (arrays whose length and element types are all literal) index precisely.
- Mutating methods are permitted on arrays the program mutates (§3.4). Assigning `length` is an error; use `splice` or `toSpliced`.
- `Map`, `Set` and `WeakMap` are the keyed collections. An object literal is never a dictionary: a shape is a fixed set of named keys. The only dictionary-typed values (`string` keys, one value type, reads `T | undefined`) are the results of `Object.fromEntries` and `Object.groupBy` and an `unknown` narrowed by `typeof x === "object" && x !== null`; they exist for interop and are read with `Object.hasOwn`/`Object.entries`, never built by hand.

### 4.7 Iteration

Any object with `[Symbol.iterator]` is iterable; iterator helpers (`map`, `filter`, `take`, `drop`, `flatMap`, `reduce`, `toArray`, `forEach`, `some`, `every`, `find`) are available on every iterator. The same holds for async iterators with `for await`.

### 4.8 Evaluation order and strictness

All ES2025 evaluation-order rules hold unchanged. Strict mode is always on (modules), so assignment to an undeclared name, writing a frozen property, and duplicate parameter names are already runtime errors; the checker reports them statically first.

## 5. Modules and the ambient environment

A Strict JS program is a closed graph of its own `.js` files; the only things it can reach besides those files are the globals that exist in both browsers and server runtimes, so there is nothing to install, declare or configure, and the checker can see the whole program.

### 5.1 Module rules

- `import { a, b } from "./x.js"` and `import * as ns from "./x.js"`. Specifiers are relative paths starting with `./` or `../` and ending in `.js`. Bare specifiers (`"lodash"`, `"node:fs"`), absolute paths and URLs are `E201 foreign-import`; a path that names no readable file is `E205 unresolved-import`.
- JSON: `import cfg from "./c.json" with { type: "json" }` is the one default-import form in the language, permitted only for a `.json` specifier with that attribute (a JSON module has no named exports). The value is `unknown` and must be narrowed. A `.json` import without the attribute, or the attribute on a `.js` import, is `E205`.
- Named exports only. `export default` is `E202 default-export`; a default import of a `.js` module is `E000`.
- Circular imports are `E203 import-cycle`, reported with the cycle. A named import or `ns.x` the target does not export is `E206 missing-export`.
- `import("./x.js")` with a literal path is permitted and resolves to the module's namespace; the graph stays static.
- `import.meta` is removed; a module has no identity at runtime.
- Top-level `await` is permitted.
- A module whose top level only declares is marked pure; a module with top-level side effects is listed in the build report so the order of evaluation is visible.

### 5.2 The ambient environment

The program sees exactly one set of globals: the intersection of ES2025 built-ins and the web-platform APIs that browsers, Node 24+, Deno 2+, Bun and Cloudflare Workers all provide (§6). Any identifier outside that set (`document`, `window`, `process`, `require`, `Buffer`, `__dirname`, `localStorage`, `Deno`, `Bun`) is `E204 unknown-global`, whether or not the runtime happens to define it.

Code that must touch a platform API lives outside the subset: a thin host file the compiler does not check imports the Strict JS modules and passes them already-constructed values (an element, a file handle, a config object). Those values are typed by how the Strict JS code uses them, exactly like any other argument.

### 5.3 Entry points

The program has one root: `./src/index.js` by default, or the entry files named on the command line. The set of modules is the transitive closure of the root's imports. Every other `.js` file beneath `./src` is `E207 unreachable-module` (delete it or import it), so what is on disk and what is the program never differ; `*.test.js` files are exempt because `sjs test` adds them as further roots, every one beneath `./src` by default or, with explicit entries, every one in the directory of each module in the graph.

## 6. Ambient library

The ambient library is the set of globals present in browsers, Node 24+, Deno 2+, Bun and Cloudflare Workers, minus the members listed as removed; the compiler carries a feature table for all five and reports any member missing on any one of them as `E204 unknown-global`, since it never emits a polyfill. There is no target selection: a program is checked against the intersection, always.

### 6.1 Kept built-ins

| Global                  | Kept members                                                                                                                                                   | Removed members                                                                                                                                                                                   |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Object`                | `keys`, `values`, `entries`, `fromEntries`, `hasOwn`, `groupBy`, `freeze`, `isFrozen`, `assign`                                                                | `defineProperty`, `defineProperties`, `getPrototypeOf`, `setPrototypeOf`, `create`, `seal`, `preventExtensions`, `is`, `getOwnPropertyNames`, `getOwnPropertySymbols`, `getOwnPropertyDescriptor` |
| `Array`                 | `isArray`, `from`, `fromAsync`, `of`, all ES2025 instance methods incl. `at`, `findLast`, `toSorted`, `toReversed`, `toSpliced`, `with`, `flat`, `flatMap`     | holes, `length` assignment, `sort`/`toSorted` without a comparator on non-string arrays                                                                                                           |
| `String`                | `raw`, `fromCodePoint`, all ES2025 instance methods incl. `at`, `replaceAll`, `isWellFormed`, `toWellFormed`                                                   | `substr`, `fromCharCode`, `charCodeAt`, HTML wrapper methods, `localeCompare` without an `Intl.Collator`                                                                                          |
| `Number`                | `isFinite`, `isInteger`, `isNaN`, `isSafeInteger`, `parseFloat`, `parseInt(s, radix)` (radix required), constants, `toFixed`, `toPrecision`, `toString(radix)` | global `parseInt`, `parseFloat`, `isNaN`, `isFinite`                                                                                                                                              |
| `BigInt`                | `asIntN`, `asUintN`, `toString(radix)`                                                                                                                         |                                                                                                                                                                                                   |
| `Math`                  | all, including `random`                                                                                                                                        |                                                                                                                                                                                                   |
| `Symbol`                | `Symbol.iterator`, `Symbol.asyncIterator`, `Symbol.dispose`, `Symbol.asyncDispose` as protocol keys                                                            | `Symbol()`, `Symbol.for`, other well-known symbols                                                                                                                                                |
| `Map`, `Set`, `WeakMap` | all, plus ES2025 set methods and `Map.groupBy`                                                                                                                 | `WeakSet`, `WeakRef`, `FinalizationRegistry`                                                                                                                                                      |
| Typed arrays            | all, `ArrayBuffer`, `DataView`, `Uint8Array.fromBase64`/`toBase64`/`fromHex`/`toHex` where the feature table allows                                            |                                                                                                                                                                                                   |
| `Promise`               | `all`, `allSettled`, `any`, `race`, `resolve`, `reject`, `withResolvers`, `try`                                                                                | `new Promise(executor)`, instance `.then`/`.catch`/`.finally` in user code                                                                                                                        |
| `Iterator`              | helpers: `map`, `filter`, `take`, `drop`, `flatMap`, `reduce`, `toArray`, `forEach`, `some`, `every`, `find`; `Iterator.from`                                  |                                                                                                                                                                                                   |
| `JSON`                  | `parse` (typed `unknown`), `stringify`, `rawJSON`, `isRawJSON`                                                                                                 | reviver/replacer functions                                                                                                                                                                        |
| `RegExp`                | literals with explicit flags incl. `v`; `exec`, `test`, `matchAll`, named groups typed from the literal                                                        | `new RegExp(string)`, `lastIndex` assignment, `compile`, legacy static props                                                                                                                      |
| `Error` and subclasses  | `message`, `cause`, `name`, `stack`, `AggregateError`                                                                                                          | `Error.captureStackTrace`                                                                                                                                                                         |
| `Intl`                  | all                                                                                                                                                            |                                                                                                                                                                                                   |
| `Date`                  | `Date.now`, `new Date(isoString)`, `new Date(ms)`, `getTime`, `toISOString`, `valueOf`, UTC getters                                                            | setters, local-time getters, string parsing other than ISO 8601, `new Date()` with no argument                                                                                                    |
| `Temporal`              | all, when the feature table marks it present on all five runtimes (as of this release it is not, so Temporal is E204 until then)                               |                                                                                                                                                                                                   |
| `structuredClone`       | typed as identity on cloneable values                                                                                                                          |                                                                                                                                                                                                   |
| `globalThis`            | removed from user code                                                                                                                                         |                                                                                                                                                                                                   |

### 6.2 Portable platform globals

`fetch`, `Request`, `Response`, `Headers`, `FormData`, `URL`, `URLSearchParams`, `URLPattern`, `AbortController`, `AbortSignal`, `EventTarget`, `Event`, `CustomEvent`, `ReadableStream`, `WritableStream`, `TransformStream`, `TextEncoderStream`, `TextDecoderStream`, `CompressionStream`, `DecompressionStream`, `Blob`, `File`, `TextEncoder`, `TextDecoder`, `WebSocket`, `BroadcastChannel`, `MessageChannel`, `MessagePort`, `crypto.subtle`, `crypto.randomUUID`, `crypto.getRandomValues`, `queueMicrotask`, `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`, `performance.now`, `performance.mark`/`measure`, `console` (`log`, `error`, `warn`, `info`, `debug`, `table`, `time`/`timeEnd`, `group`/`groupEnd`, `assert`), `navigator.hardwareConcurrency`, `atob`/`btoa`.

Each of these is typed from the WHATWG/WinterCG IDL with `null` kept where the platform returns it (narrow with `=== null`) and callbacks typed by their parameters. Values that are `any` in IDL (a `postMessage` payload, `JSON.parse`'s result, `event.data`) are `unknown`.

### 6.3 Boundary narrowing

There is no schema library; `unknown` values are narrowed with ordinary checks, and the checker tracks them precisely: after `typeof x === "object" && x !== null && "name" in x` the program would need `in`, which is removed, so the idiom is `Object.hasOwn(x, "name")` followed by `typeof x.name === "string"`. A helper function built from such checks becomes a predicate (§3.5) and is reused across the program. The compiler ships no runtime code for this; the helper is the programmer's own file.

### 6.4 Removed globals

`eval`, `Function`, `Proxy`, `Reflect`, `WeakRef`, `FinalizationRegistry`, `escape`/`unescape`, `encodeURI`/`decodeURI` (keep `encodeURIComponent`/`decodeURIComponent`), `globalThis`, `self`, `window`, `global`, `document`, `navigator` members other than `hardwareConcurrency`, `localStorage`/`sessionStorage`/`indexedDB`, `Worker`, `SharedArrayBuffer`/`Atomics`, `XMLHttpRequest`, `require`, `module`, `exports`, `process`, `Buffer`, `__dirname`, `__filename`, `Deno`, `Bun`.

## 7. Build output

There is no lowering: the files the programmer wrote are the files that run, and `sjs build` copies them unchanged after a successful check so a diff between source and output is always empty.

**What `sjs build` produces**

| Output             | Contents                                                                                                                                                                                      |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dist/**/*.js`     | byte-for-byte copies of the checked modules, same relative paths                                                                                                                              |
| `dist/report.json` | every diagnostic (errors and style findings), the inferred public signature of each exported binding, the list of impure modules in evaluation order, the ambient globals each module touches |
| `dist/types.json`  | the full inferred type of every exported binding, for editors and for reviewers who want to read a signature without reading a body                                                           |

**What it never does**

- Never rewrites an expression (`|0`, constant folding, dead-code removal). Those change what runs.
- Never bundles, minifies or concatenates. A bundler is the programmer's host-side choice; the modules are standard ESM and any bundler accepts them.
- Never injects a polyfill. A global missing on any of the five runtimes is an error at check time.
- Never generates source maps, because there is nothing to map.
- Never leaves a partial `dist/`: on any E-code the output directory holds `report.json` alone, and the directory is written under a temporary name and renamed into place.

**Runtime invariants (checked by the conformance suite).** The emitted tree is identical to the input tree; a program that passes the checker produces identical observable behavior on every target in the feature table; and no checked program references a removed global.

## 8. Linting rules

The linter has two severities: `error` (E-codes, fail the build) and `style` (S-codes, reported with the exact replacement text and applied only by an explicit `sjs format --write`). Nothing is configurable per project, so every codebase lints identically, and nothing is ever changed without the programmer seeing the diff first.

### 8.1 Correctness errors

| Code      | Rule                                                                                                                                                                                                                         |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E000      | syntax: any construct outside the §2 grammar, including braceless bodies, hashbang lines, `++` as a value, and ES2025 early errors; the message names the §12 replacement                                                    |
| E001      | ambiguous-line-start (ASI hazard, incl. a split `return`/`throw`/`yield` operand); hand edit, no proposed fix                                                                                                                |
| E002      | regex-flags: flags missing, `v` missing, or a flag-dependent method on a regex without that flag                                                                                                                             |
| E100      | null-produced: program creates or returns `null`                                                                                                                                                                             |
| E101      | excess-property on an object literal                                                                                                                                                                                         |
| E102      | ambiguous-truthiness                                                                                                                                                                                                         |
| E103      | non-exhaustive switch / if-chain                                                                                                                                                                                             |
| E104      | bitwise-on-double                                                                                                                                                                                                            |
| E105      | no-overlap equality                                                                                                                                                                                                          |
| E106      | unused-let                                                                                                                                                                                                                   |
| E107      | block-shadow                                                                                                                                                                                                                 |
| E108      | switch fallthrough                                                                                                                                                                                                           |
| E109      | finally-return                                                                                                                                                                                                               |
| E110      | empty-array-untyped                                                                                                                                                                                                          |
| E111      | mixed-return                                                                                                                                                                                                                 |
| E112      | unconstrained-parameter on a non-exported function                                                                                                                                                                           |
| E113      | floating-promise                                                                                                                                                                                                             |
| E114      | uninitialized-field                                                                                                                                                                                                          |
| E115      | unused-binding (import, variable, parameter, catch binding; `_` prefix opts out)                                                                                                                                             |
| E116      | unused-expression (statement with no effect)                                                                                                                                                                                 |
| E117      | unreachable-code                                                                                                                                                                                                             |
| E118      | async-no-await                                                                                                                                                                                                               |
| E119      | await-non-promise                                                                                                                                                                                                            |
| E120      | self-assign / self-compare                                                                                                                                                                                                   |
| E121      | constant-condition: if (true), if on a const false, while (true) whose body has no break/return/throw (incl. `?.` on a receiver that is never nullish)                                                                       |
| E122      | write-to-frozen                                                                                                                                                                                                              |
| E123      | modified-loop-variable                                                                                                                                                                                                       |
| E124      | array-index-unchecked                                                                                                                                                                                                        |
| E125      | template-non-string                                                                                                                                                                                                          |
| E126      | this-outside-class                                                                                                                                                                                                           |
| E127      | promise-executor (`new Promise`)                                                                                                                                                                                             |
| E128      | then-chain                                                                                                                                                                                                                   |
| E129      | shape-change: a class instance or plain object gaining or losing a key after creation                                                                                                                                        |
| E130      | throw-non-error                                                                                                                                                                                                              |
| E131      | catch-untyped-use                                                                                                                                                                                                            |
| E132      | commented-out code: a non-doc comment whose text (consecutive // lines joined, a leading \* per line stripped) parses under §2 as a statement or declaration and contains at least one of ( { \[ = =>; prose never qualifies |
| E133      | removed-global / removed-member used (incl. comparator-less `sort` on non-string arrays)                                                                                                                                     |
| E134      | duplicate-key in an object literal                                                                                                                                                                                           |
| E135      | use-before-declaration (temporal dead zone)                                                                                                                                                                                  |
| E136      | arity-mismatch                                                                                                                                                                                                               |
| E137      | missing-property: a key not present on every member of the receiver's type                                                                                                                                                   |
| E138      | type-mismatch: any assignability violation without a more specific code, incl. assignment to a `const`, import, function or class binding, and an incompatible override                                                      |
| E139      | not-callable                                                                                                                                                                                                                 |
| E140–E145 | size limits, §8.2                                                                                                                                                                                                            |
| E150–E153 | naming, §8.3                                                                                                                                                                                                                 |
| E160–E163 | structure, §8.4                                                                                                                                                                                                              |
| E201–E207 | module and ambient rules, §5; E205 unresolved-import, E206 missing-export, E207 unreachable-module                                                                                                                           |
| E301      | test-no-assert                                                                                                                                                                                                               |
| E302      | test-focused-committed (`it.only`/`describe.only` under `--ci`)                                                                                                                                                              |
| E303      | test-async-no-await                                                                                                                                                                                                          |
| E304      | test-skipped-without-reason (under `--ci`)                                                                                                                                                                                   |
| E401–E403 | doc comments, §13.7                                                                                                                                                                                                          |

### 8.2 Complexity and size limits (errors, fixed)

| Code | Limit                 | Value and measure                                                                                                                                                                                       |
| ---- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E140 | Function length       | 60 lines inside the body braces, excluding blank and comment-only lines                                                                                                                                 |
| E141 | Cyclomatic complexity | 12 per function: 1 plus one for each `if`, `else if`, `for`, `for…of`, `while`, `case` label, `catch`, `&&`, `\|\|`, `??`, `??=`, `?:` and `?.`; nested function and arrow bodies start their own count |
| E142 | Nesting depth         | 4 blocks inside the function body (the body itself is 0); nested function bodies restart                                                                                                                |
| E143 | Parameters            | 4; a rest parameter or a destructured options object counts as one                                                                                                                                      |
| E144 | File length           | 600 lines, counting blank and comment lines                                                                                                                                                             |
| E145 | Class members         | 20: fields, methods, accessors, static members and static blocks, `#private` included                                                                                                                   |

### 8.3 Naming (errors)

- **E150 naming-case.** Values, functions, parameters, fields: camelCase `^[a-z][A-Za-z0-9]*$` (acronym runs such as `parseURL` allowed). Classes: PascalCase `^[A-Z][A-Za-z0-9]*$`. A module-level `const` whose inferred type is a primitive or a literal of one: UPPER_SNAKE `^[A-Z][A-Z0-9]*(_[A-Z0-9]+)*$`; every other `const` is camelCase. `#private` names follow the rule of their public counterpart after the `#`.
- **E151 boolean-name.** A binding, parameter, field or function whose inferred type (or return type) is exactly `boolean` starts with `is`, `has`, `can` or `should` followed by an uppercase letter or digit, and only such names carry that prefix. `boolean | undefined` is not covered.
- **E152 file-name.** `^[a-z0-9]+(-[a-z0-9]+)*(\.test)?\.js$`. A module whose only export is a class is named by that class in kebab-case, splitting a run of capitals before its last capital (`HTTPClient` → `http-client.js`).
- **E153 underscore-prefix.** A `_`-prefixed binding, or `_` itself, that is read.

### 8.4 Structure (errors)

- **E160 mixed-exports.** A module that exports a class exports nothing else; a module exports at most one class. Otherwise it exports a cohesive set of functions and constants.
- **E161 export-star.** `export * from`.
- **E162 missing-doc-comment.** An exported declaration without a `/** */` block on the lines immediately above it, no blank line between (a blank line turns the block into a floating comment).
- **E163 todo-without-issue.** A comment containing `TODO` (case-sensitive) that does not also contain `#<digits>` or an `http://`/`https://` URL.

### 8.5 Style findings (S-codes, reported, never auto-applied)

Each finding names the file, line, the current text and the text the formatter would write.

| Code | Finding                                                                                            |
| ---- | -------------------------------------------------------------------------------------------------- |
| S001 | single-quoted string                                                                               |
| S002 | semicolon present (the canonical style omits them)                                                 |
| S003 | import groups or specifiers out of order                                                           |
| S004 | trailing comma missing in a multi-line list / present in a single-line one                         |
| S005 | redundant parentheses                                                                              |
| S006 | line longer than 100 columns                                                                       |
| S007 | indentation not a multiple of 2 spaces, or a tab character                                         |
| S008 | any other §9.1 layout rule: blank-line placement, line endings, trailing whitespace, final newline |
| S009 | class members out of canonical order                                                               |
| S010 | method chain of 3+ calls not broken one per line                                                   |
| S011 | arrow with a block body that is a single `return`                                                  |
| S012 | `i++`/`i--` as a statement outside a `for` header (replacement `i += 1`)                           |
| S020 | `const` object never mutated and never frozen (candidate for `Object.freeze`)                      |
| S021 | exported function whose inferred signature is wider than any call uses                             |
| S030 | call to an export marked `@deprecated`                                                             |

S020 and S021 are the two style findings that would change behavior if applied (freezing throws on later writes; narrowing a signature changes what compiles); the report says so beside them, and `sjs format --write` never applies them even when asked, so they are always a hand edit.

## 9. Formatting rules

The formatter defines one canonical layout for any AST and, by default, only reports where a file departs from it; `sjs format` prints the diff, `sjs format --write` applies it, and every change it can make is whitespace, quotes, commas, parentheses or ordering, so applying it never changes behavior.

### 9.1 Layout

- Indent: 2 spaces. Tabs never appear.
- Line width: 100 columns, hard. A single token longer than 100 (a long string) is left on its own line.
- Line endings `\n`; file ends with exactly one `\n`; no trailing whitespace.
- One blank line between top-level declarations; at most one consecutive blank line anywhere; none at the start or end of a block.
- Imports grouped: relative paths sorted by depth then name; specifiers sorted, one per line once the import exceeds the width.

### 9.2 Punctuation

- No semicolons; E001 guarantees the file parses identically with or without ASI, so an ordinary engine reads it the same way.
- Double quotes for strings; a string containing `"` keeps double quotes with escapes.
- Trailing commas in every multi-line list, object, parameter list and import; none in single-line forms.
- Parentheses: always around arrow parameters; always around a `??` beside `&&`/`||`; otherwise only where precedence requires.
- Spaces around binary operators and after `,` `:`; none inside `()` `[]`; one inside `{ }` for single-line objects, imports and destructuring; none for empty `{}`.

### 9.3 Breaking

- Objects, arrays, arguments and parameters are single-line if they fit; otherwise every element goes on its own line.
- An object literal with a newline between `{` and the first key in the source stays multi-line (the one layout intention honored).
- Method chains of three or more calls break before each `.`, one call per line.
- Binary expressions break after the operator; ternaries break before `?` and `:` with both branches indented.
- Arrow bodies: expression bodies stay expressions; an object-literal body is written `() => ({ … })`.
- Template literals are never reflowed.

### 9.4 Declarations

- Class members are ordered: static fields, static methods, instance fields (public then `#private`), constructor, accessors, methods (public then `#private`); within a group, source order is preserved.
- A `function` declaration has one blank line above its preceding comment.

### 9.5 Comments

- `//` comments have one space after the slashes and sit on their own line above the code, or at end of line with two spaces before.
- `/** */` doc comments are reflowed to the line width.
- Comments are never moved across declarations.

### 9.6 Reporting

`sjs lint` includes formatting deviations as S-codes (§8.5) with before/after text. `sjs format` prints a unified diff per file and exits non-zero if any file would change. `sjs format --write` applies that diff and nothing else: it never touches S020/S021 and never runs as part of `lint`, `build` or `test`.

## 10. Testing

A test file is a plain `.js` module that imports nothing but the code under test and uses the `describe`, `it`, `beforeEach`, `afterEach` and `expect` globals; those globals are what Jest, Vitest (`globals: true`), Bun and the Strict JS runner all provide, so one file runs under any of them, and the Strict JS checker types every assertion against the inferred type of its subject.

### 10.1 File conventions

- Test files end in `.test.js` and sit beside the module under test.
- A test file may import only project modules. The test globals are ambient in test files only; using them in a non-test file is `E204 unknown-global`.
- `sjs test` discovers test files as §5.3 says, checks them with the program, and runs each file in an isolated module graph (a worker thread per file) on one JavaScript runtime. That runtime is the one that launched `sjs` when it was started through a runtime's package runner (`npx`, `deno run`, `bunx`); otherwise the first of `node` (24+), `deno` (2+), `bun` found on `PATH`. None found is a usage error (exit 2). The summary names the runtime used. There is no target selection: the checker has already proven the program against the five-runtime intersection, so running it twice would prove nothing the check did not.
- The `describe`/`it`/`expect` implementation the runner injects is a host file outside the subset (it must install globals, which the subset forbids); its matcher logic is kept in a Strict JS module that `sjs lint` checks, and only the few lines that attach names to the global object live outside.

### 10.2 Ambient test API (the portable intersection)

```
describe(name, body)
it(name, body)                    // body: () => void | Promise<void>
it.skip(name, body)  it.only(name, body)   describe.skip / describe.only
beforeEach(fn)  afterEach(fn)  beforeAll(fn)  afterAll(fn)

expect(actual)
  .toBe(expected)                 // ===
  .toEqual(expected)              // deep structural equality
  .toStrictEqual(expected)        // deep equality incl. undefined keys and class identity
  .toBeUndefined()  .toBeDefined()
  .toBeNull()                     // only for host-produced values typed to include null
  .toBeTruthy()  .toBeFalsy()     // permitted only on boolean subjects (E102 otherwise)
  .toBeGreaterThan(n) .toBeLessThan(n) .toBeGreaterThanOrEqual(n) .toBeLessThanOrEqual(n)
  .toBeCloseTo(n, digits)
  .toContain(item)                // arrays and strings
  .toHaveLength(n)
  .toMatch(regex)                 // regex literal with explicit flags
  .toBeInstanceOf(Class)
  .toThrow(ErrorClass)            // subject must be a zero-arg function
  .resolves / .rejects            // subject must be a promise
  .not
```

Every matcher is checked against the inferred type of `actual`: `expect(5).toBeUndefined()`, `expect("x").toBeCloseTo(1)` and `expect(obj).toHaveLength(2)` on an object without `length` are errors. `toEqual` understands `Map`, `Set`, typed arrays, `Date` and class instances. Matchers outside this list (`toMatchSnapshot`, `toHaveBeenCalled`, `toMatchObject`) are not ambient; using one is `E204`.

### 10.3 Doubles without a mocking library

There is no `jest.fn`, `vi.fn` or module mocking, because none of those is portable or pure JavaScript. The pattern the checker supports instead:

- A fake is a plain function or class instance written in the test file; it records calls in an array the test asserts on. Because the fake is passed to the code under test as an argument, its shape is checked against what the code actually calls.
- Time, randomness and I/O are injected: a module that needs `now()` or `random()` takes them as parameters (or as fields of an options object) with the real `Date.now`/`Math.random` passed by the host file. Tests pass deterministic functions.
- A module that reads a global directly (`fetch`, `setTimeout`) is still testable by assigning a fake to that global in `beforeEach` and restoring it in `afterEach`; the checker allows reassigning an ambient global only inside a `.test.js` file and only to a value of the global's inferred type.

### 10.4 Runner semantics (`sjs test`)

- Tests in a file run sequentially in declaration order; files run in parallel.
- Default timeout 5 000 ms per test; a test that exceeds it fails and its promise is abandoned.
- `it.only`/`describe.only` narrow a local run and are E302 under `--ci`. `it.skip`/`describe.skip` without a `//` reason on the same or the preceding line is E304 under `--ci`.
- Matchers are typed against the narrowed type of the subject at the call site: `expect(x).toBe(1)` on `number | undefined` is E138 until `x` is narrowed.
- Output: TAP 14 by default; `--reporter=dots|junit|json`. The summary names the runtime the tests ran on. Exit code is the number of failed files, capped at 255.

### 10.5 Coverage

Coverage is on by default. `sjs test` instruments a copy of each module in memory (statement, branch, function), never the files on disk, and maps results to source lines; `--no-coverage` runs the exact source bytes instead. Thresholds (`--coverage-lines=90 --coverage-branches=85`) are per file, so a file with no tests cannot hide inside an aggregate; the aggregate is printed as well. They are enforced under `--ci`. Output goes to `--out` (default `coverage/`): `index.html` with a per-file table, one annotated-source page per file with uncovered lines and branches marked, and `coverage.json` for CI tooling.

### 10.6 Checking the types of exports

Because there are no annotations, a library tests its public types by use: a test that calls an export with a wrong argument is itself a compile error, which is the assertion. `sjs lint --signatures` prints the inferred signature of every export so a reviewer can diff it between commits; a widened or narrowed signature shows up as a text change.

## 11. Compiler architecture and roadmap

The compiler is a single binary (`sjs`) with six passes over one immutable AST that it never rewrites; the formatter and the test runner hang off that AST, so every tool sees the same program the engine will run.

&#91;embedded content: compiler pipeline · 6 passes, 2 side tools, no emitter\]

Each pass is a pure function from the previous pass's output. Inference and checking are separate so that `types.json` can be produced even when the checker fails, and so the reporter can print a signature next to every error.

### 11.1 Implementation choices

- Host language: Rust, with the AST in an arena and types interned. Whole-program inference is incremental: each module's summary (exports' inferred types, globals touched, mutations performed) is keyed on content hash, and watch mode re-infers only the changed file and its dependents until the summaries stop changing.
- Parser: hand-written recursive descent producing a lossless syntax tree (comments and whitespace kept) so the formatter can report exact before/after text.
- Inference: bidirectional with let-polymorphism; unions are normalized sets; narrowing is a flow analysis over a per-function CFG; call-site types are collected program-wide before bodies are checked, so an exported function's parameters are typed by its real callers.
- No emitter. `sjs build` copies files and writes the two JSON reports.
- CLI: `sjs lint`, `sjs build`, `sjs format`, `sjs format --write`, `sjs test`, `sjs map, sjs docs, sjs help`, `sjs lsp`. The language server reuses the incremental inference and serves inferred types as hover text.

### 11.2 Roadmap

1. Milestone 0: parser and formatter. Parse and round-trip the whole §2 grammar; formatter idempotence (`fmt(fmt(x)) === fmt(x)`) proven on a corpus of 10 000 generated programs; S-code reporting with exact diffs.
2. Milestone 1: inference core. Primitives, literals, unions, objects, arrays, functions, generics, mutability inference; conformance suite of positive and negative programs per rule in §3–4.
3. Milestone 2: checker. Narrowing, exhaustiveness, shape stability, `int32`, regex flags, the ambient-global feature table for all five runtimes.
4. Milestone 3: linter and reporter. Every E- and S-code in §8, `report.json`, `types.json`, exit codes, `--ci`.
5. Milestone 4: test runner. Ambient test API typing, single-runtime execution, coverage, TAP output; the compiler's own tests pass unchanged under Jest, Vitest and Bun as the portability proof.
6. Milestone 5: adoption. Language server; a port of one real module from each of your two teams' codebases as the acceptance test, with the host-file boundary (§5.2) documented from that experience.

### 11.3 Acceptance criteria

- Every example in this spec passes the checker or fails with the named code.
- `dist/` is byte-identical to the checked sources on every build.
- Every test file in the conformance suite produces the same pass/fail set under `sjs test`, Jest, Vitest and Bun.
- Inference throughput at least 100 000 lines per second per core on the self-hosted corpus, with incremental re-check of a one-file edit under 100 ms on a 50 000-line program.

## 12. Appendix: removed features and rationale

Each removal is either a duplicate of a kept construct or a hole in static inference; the replacement column is the one spelling that survives, and every replacement is itself ordinary JavaScript.

| Removed                                                                         | Reason                                                        | Replacement                                          |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------- | ---------------------------------------------------- |
| `var`                                                                           | function scoping, hoisted `undefined`                         | `const` / `let`                                      |
| `==`, `!=`                                                                      | coercion                                                      | `===`, `!==`                                         |
| `null` as a value you create                                                    | second nullish value, `typeof null === "object"`              | `undefined`; `=== null` only to narrow host values   |
| `with`                                                                          | dynamic scope                                                 | none                                                 |
| labels                                                                          | goto-like control                                             | restructure with functions                           |
| comma operator                                                                  | hidden sequencing                                             | separate statements                                  |
| `void`                                                                          | expression-to-undefined                                       | `undefined`                                          |
| `delete`                                                                        | shape change                                                  | set to `undefined`, or `Map.delete`                  |
| `in`                                                                            | prototype-walking key test                                    | discriminants, `Object.hasOwn`                       |
| `for…in`                                                                        | iterates inherited keys as strings                            | `for…of Object.keys/entries`                         |
| `do…while`                                                                      | duplicate of `while`                                          | `while`                                              |
| `switch` fallthrough                                                            | silent bugs                                                   | stacked `case` labels on one block                   |
| function and class expressions                                                  | duplicates of arrows and declarations                         | arrow / `function` / `class` declarations            |
| `arguments`                                                                     | untyped, array-like                                           | rest parameters                                      |
| `.call`/`.apply`/`.bind`                                                        | rebinding `this`                                              | closures, class methods                              |
| `this` outside classes                                                          | dynamic receiver                                              | explicit parameters                                  |
| `new.target`, `prototype`, `__proto__`, `Object.create`, `setPrototypeOf`       | prototype manipulation                                        | classes                                              |
| `Proxy`, `Reflect`, `defineProperty`                                            | runtime shape changes, uninferrable                           | plain classes and objects                            |
| user `Symbol`                                                                   | ad-hoc privacy and protocols                                  | `#private`, well-known symbols only                  |
| `eval`, `Function`, `new RegExp(string)`                                        | code from strings                                             | none; regex literals                                 |
| getters/setters/methods on object literals                                      | hidden computation in literals                                | classes                                              |
| \`                                                                              |                                                               | =`, `&&=\`                                           |
| `\|\|`/`&&` returning operands                                                  | truthiness coercion                                           | `??`, `?:`, explicit comparisons                     |
| bitwise ops on doubles                                                          | silent int32 truncation                                       | inferred `int32` operands, `bigint`                  |
| array holes, `length` assignment                                                | sparse arrays, deopts                                         | dense arrays, `splice`/`toSpliced`                   |
| `new Promise(executor)`                                                         | swallowed throws, resolve/reject footguns                     | `Promise.withResolvers`, `Promise.try`               |
| `.then`/`.catch`/`.finally`                                                     | duplicate of `await`/`try`                                    | `async`/`await`                                      |
| `export default`                                                                | anonymous, renamed on every import                            | named exports                                        |
| bare, absolute and URL imports; CommonJS                                        | open world, platform-specific                                 | relative `.js` imports only                          |
| ASI                                                                             | ambiguous line breaks                                         | no-semicolon layout guarded by E001                  |
| decorators                                                                      | unsettled semantics, hidden mutation                          | explicit wrapping functions                          |
| regex without explicit flags / without `v`                                      | legacy syntax, implicit behavior                              | flags always written, `v` required                   |
| `let` without initializer                                                       | implicit `undefined`                                          | `let x = undefined` when meant                       |
| `import.meta`                                                                   | module identity at runtime                                    | none                                                 |
| `Date` setters and local-time API                                               | mutation, zone ambiguity                                      | UTC getters, `toISOString`, `Temporal` where present |
| `globalThis`, `window`, `self`, `process`, `document`, `localStorage`, `Worker` | present on one side only                                      | host file outside the subset                         |
| `WeakRef`, `FinalizationRegistry`                                               | non-deterministic                                             | none                                                 |
| `Object.seal`/`preventExtensions`                                               | partial immutability                                          | `Object.freeze`                                      |
| tagged templates except `String.raw`                                            | untyped DSLs                                                  | functions taking strings                             |
| braceless statement bodies                                                      | two spellings of one construct; dangling-else and ASI hazards | a block in braces, always                            |
| #! hashbang line                                                                | server-only signal                                            | a host-side launcher outside the subset              |
| object literal as a dictionary                                                  | open key set defeats shape inference                          | Map; Object.fromEntries/groupBy for interop only     |
| ++/-- as a value                                                                | pre/post ordering hidden in an expression                     | i += 1 on its own line; ++ only in a for header      |

**What was kept that is often debated**

- Classes: the only mechanism with fixed shapes, nominal typing and native privacy; kept.
- Generators: no duplicate exists for lazy sequences; kept with iterator helpers.
- `switch`: kept because exhaustiveness checking makes it the typed pattern-match; fallthrough removed.
- Classic `for` loop: kept for index iteration where `for…of` cannot prove range for indexing.
- `instanceof`: kept as the nominal class test; it is the only runtime type check that cannot be faked.
- `Object.freeze`: kept as the one runtime immutability marker, because it is the only way to express readonly without an annotation.
- `Math.random`, `Date.now`, `setTimeout`: kept as globals; tests substitute them by injection or by reassignment inside a test file (§10.3).

## 13. Command-line interface

`sjs` has eight commands, seven for people and one for editors; every one takes the same entry files, runs the same passes up to the point it needs, exits non-zero on any E-code, and writes nothing to the source tree unless the command's purpose is to write.

### 13.1 Commands

| Command                                                                                                | Passes run                            | Writes                                                                      | Exit code                              |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------- | --------------------------------------------------------------------------- | -------------------------------------- |
| `sjs build [entries…] [--out dist]`                                                                    | all six                               | byte-identical copies of the sources, `report.json`, `types.json`           | 0 if no E-code, else 1                 |
| `sjs lint [entries…] [--signatures] [--json]`                                                          | all six, no copy                      | nothing                                                                     | 0 if no E-code, else 1                 |
| `sjs format [entries…] [--write] [--check]`                                                            | parser only                           | with `--write`: the canonical layout, S020/S021 excluded; otherwise nothing | files that would change, capped at 255 |
| `sjs test [entries…] [--no-coverage] [--ci] [--reporter …] [--coverage-lines N --coverage-branches N]` | all six, then the runner              | coverage report under `--out`                                               | failed files, capped at 255            |
| `sjs map [entries…] [--format html\|json\|dot\|md] [--out map]`                                        | parser, binder, inference             | the map in the chosen format                                                | 0 if no E-code, else 1                 |
| `sjs docs [entries…] [--format html\|md] [--out docs]`                                                 | all six                               | the generated site or markdown tree                                         | 0 if no E-code, else 1                 |
| `sjs help [command]`                                                                                   | none                                  | nothing                                                                     | 0                                      |
| sjs lsp                                                                                                | all six, incremental, on stdin/stdout | nothing; edits are returned to the editor, never applied                    | 0 on clean shutdown                    |

Common flags: `--out <dir>` (default `dist/`, `map/`, `docs/` per command), `--json` (machine-readable diagnostics), `--quiet`. There is no `--target` flag anywhere: every command checks against the intersection of the five runtimes. The root defaults to `./src/index.js` plus every `*.test.js` beneath `./src` for `test`. A usage error (unknown command or flag, missing entry file, out-of-range value) exits 2 and prints the command's help; an internal failure exits 70; diagnostics alone never produce either. Lines and columns in diagnostics are 1-based, columns counting Unicode scalar values, with UTF-8 byte offsets alongside in `report.json`.

### 13.2 `build`

Runs the full check and copies the sources (§7). `--out` is the only option. A build with E-codes writes `report.json` and nothing else, so a partial `dist/` never exists.

### 13.3 `lint`

The full analysis without copying: every E-code and S-code with file, line, column, current text and the formatter's replacement. `--signatures` appends the inferred signature of every export in a stable text form, one per line, for diffing between commits. `--json` emits the same as `report.json` on stdout.

### 13.4 `format`

Default behavior prints a unified diff per file that departs from §9 and exits with the count of such files, so CI can gate on it without touching anything. `--write` applies exactly that diff. `--check` is the default spelled out. The command never applies S020 (freeze candidate) or S021 (narrowable signature) because those change behavior; they appear in `lint` only.

### 13.5 `test`

Coverage is on by default: each module is instrumented in memory (statement, branch, function), results map to source lines, and a summary prints after the TAP stream with an HTML report in `--out`. `--no-coverage` skips instrumentation (faster, and the only mode in which the exact source bytes execute). Thresholds (`--coverage-lines`, `--coverage-branches`) are enforced only under `--ci`; `--ci` also turns `it.only`, `describe.only` and skips without a reason into errors. Tests run on the runtime where `sjs` is invoked and the summary names it; there is no multi-runtime mode (§10.1). `--reporter` is `tap` (default), `dots`, `junit` or `json`.

### 13.6 `map`

Produces an architectural map from the binder and inference passes, with no execution. The map has four layers, each derivable from the program alone:

1. **Modules**: every file, its exports with inferred signatures, whether it is pure (declarations only) or has top-level effects, and which ambient globals it touches.
2. **Dependencies**: the import graph as a DAG (cycles are already E203), with each module's depth from the entries; `map` lays modules out by depth so the layering of the codebase is visible without configuration.
3. **Types**: every class with its fields, methods, and `extends` chain; every structural shape that two or more exports share (inferred, not declared), named by the properties it carries.
4. **Calls**: for each exported function, the exported functions it calls transitively, so a reader can see what a public entry point reaches.

Formats: `html` (default) is a single self-contained page with the dependency graph drawn by depth, click-through to each module's exports and signatures, and the type and call layers as tabs; `json` is the same data for other tools; `dot` is the dependency and class graphs for Graphviz; `md` is one markdown file per module with a table of exports and a list of importers and imports, suitable for committing beside the code.

### 13.7 `docs`

Generates reference documentation from doc comments and inferred types. Because the language has no annotations, the comment structure carries only prose; every type, signature, and default value comes from inference, so documentation can never disagree with the code.

**Doc comment structure**

A doc comment is a `/** */` block directly above an exported declaration, or at the top of a file before any code (the module comment). Its body is Markdown with these rules:

- The first paragraph is the summary; it appears in indexes and hover text.
- Following paragraphs are the description.
- Tags start a line with `@` and take no types:

| Tag           | Form                                  | Checked by the compiler                                                                                                                                                                                        |
| ------------- | ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `@param`      | `@param name  prose`                  | `name` must be a parameter of the declaration, or options for a destructured parameter, with optional @param options.key lines naming destructured keys; every parameter must have one on an exported function |
| `@returns`    | `@returns prose`                      | present iff the function returns a value                                                                                                                                                                       |
| `@throws`     | `@throws ErrorClass  prose`           | `ErrorClass` must be a class the body throws with throw new, directly or through project functions it calls (transitively, from the call graph); errors from ambient APIs are not tracked                      |
| `@field`      | `@field name  prose`                  | on a class comment, `name` must be a public field; every public field must have one                                                                                                                            |
| `@example`    | a fenced `js` block following the tag | the block is compiled as a test file in the module's scope and run by `sjs test`; an example with no assertion is E301 test-no-assert, because an example is a test; it needs at least one `expect`            |
| `@see`        | `@see ./other.js#exportName`          | the target must exist; rendered as a link                                                                                                                                                                      |
| `@deprecated` | `@deprecated prose`                   | call sites of the export are reported as S030                                                                                                                                                                  |
| `@since`      | `@since text`                         | none; rendered                                                                                                                                                                                                 |

A tag the table does not list is `E401 unknown-doc-tag`. A `@param` naming a parameter that does not exist, or an exported function missing a `@param`, is `E402 doc-mismatch`. Doc comments on non-exported declarations are permitted and rendered in a collapsed "internal" section.

**Generated output**

- One page per module: the module comment, then each export with its inferred signature (rendered from `types.json`, including literal unions and discriminants), the prose, the examples with their test result (pass or fail), importers of the module, and the modules it imports.
- An index page: the dependency layering from `map`, every export with its summary, and a search box (html only).
- Inferred signatures use a readable notation: `greet(name: string, opts: { loud: boolean }): string`, `parse(text: string): { ok: true, value: Config } | { ok: false, error: ParseError }`. The notation is output only; it is never accepted as input anywhere.
- `md` output is one file per module plus `index.md`, designed to be committed and viewed in a repository browser; `html` is a self-contained static site.

Because `@example` blocks are real tests, `sjs test` includes them and `sjs docs` shows their status; an example that fails to compile fails the docs build.

**Examples stay inside the subset.** A doc comment is ignored by every engine, so a file carrying examples is still plain JavaScript to any runtime or test runner; only `sjs` reads the fence. The example code is ordinary Strict JS in the module's scope: no imports, `expect` as its only extra global, the leading `*` of each comment line stripped before parsing. The one constraint comes from JavaScript itself: the characters `*/` end the comment, so an example may not contain a regex such as `/a*/v`, a string such as `"**/*.js"`, or a nested block comment. The checker reports such an example as `E403 example-terminates-comment` with the offending position, rather than letting the comment close early and the rest of the example become code.

### 13.8 `help`

`sjs help` lists the commands with one line each; `sjs help <command>` prints that command's flags, defaults and exit codes, and the section of this spec that defines it. Every other command accepts `--help` as a synonym.

### 13.9 `lsp`

`sjs lsp` speaks the Language Server Protocol over stdin/stdout and reuses the incremental inference. It serves: diagnostics (every E- and S-code, each S-code carrying its replacement text as a code action); hover with the inferred type in the §13.7 notation; go-to-definition; find-references; document and workspace symbols (the §13.6 module and export layers); and signature help. A code action applies exactly one S-finding and only when the editor sends the request for it, so an editor's format-on-save setting is the programmer's explicit choice, equivalent to `format --write`. It never applies S020/S021, never applies edits on its own initiative, and performs no rename or refactoring beyond returning text edits the editor applies.
