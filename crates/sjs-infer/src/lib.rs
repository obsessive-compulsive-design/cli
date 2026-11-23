//! Whole-program type inference (SPEC.md §3). US-083 begins it with the expressions
//! whose type is plain from how they are written: literals, templates, `??`, the
//! results of `JSON.parse` and of a JSON import. [`Typer::type_of`] gives the type of
//! an expression when it can be told yet; [`check_expressions`] reports `null` written
//! where a program may not write it (E100) and a template filled with a value that is
//! no text (E125). US-084 adds [`infer_bindings`]: the type of each `const` and `let`,
//! through destructuring too. US-085 completes object literals (spreads, computed keys,
//! E000, E134 and E138 for what they may not hold) and adds [`infer_program`]: the
//! modules in order, so that an import has the type of what it is bound to and a
//! namespace the shape of its module's exports. US-086 types property reads and judges
//! them and property writes (E137, E121, E129, E138). US-087 follows what the program mutates
//! and what fills an empty collection: a `const` array literal nothing mutates is a
//! tuple, an empty `[]`, `new Map()` or `new Set()` is typed by what is put into it and
//! is E110 when nothing is, and assigning `length` is E133. US-088 types the operators
//! of SPEC.md §4.1 and judges their operands (E138, E104, E105, E120). US-089 infers the
//! signatures of functions declared by name, from their bodies and from every call
//! of them, and judges calls (E111, E136, E139). US-090 generalizes: a parameter the body
//! treats opaquely is a type parameter, each call its own instance (E112 when nothing
//! says what a parameter is). US-091 types callbacks by what they are expected to be: an
//! arrow passed, stored in a slot that has a type or returned beside other functions takes
//! its parameters' types from there. US-092 types classes: fields, methods, accessors,
//! constructors and static members, `extends`, and classes generic in what their
//! constructor is passed (E114, E129, E137, E138). US-093 types iteration: what
//! `for…of`, a spread and the iterator helpers take from arrays, strings, `Map`s, `Set`s,
//! generators and iterable classes (E138 for what is no iterable). US-094 types `await`
//! and the statics of `Promise`, and judges promises (E113, E118, E119, E127, E128).
//! US-095 types the statics of `Object` and `Array` and the methods of arrays (E133 for
//! a `sort` without a comparator, E138 for a search for what an array cannot hold).
//! US-096 types the built-ins of strings, numbers and bigints, `Math`, `JSON` and
//! `structuredClone` (E133 for their removed members, E138 for a clone of a class
//! instance). US-097 types `Map`, `Set` and `WeakMap` and their methods, and the typed
//! arrays, `ArrayBuffer` and `DataView` as classes of the ambient library. US-098 adds
//! `RegExp`, `Date`, the `Error` family and the constructors of `Intl` to those classes,
//! with what a regex literal captures (E133 for their removed members and makings).

mod ambient;
mod bindings;
mod builtins;
mod classes;
mod collections;
mod context;
mod expressions;
mod functions;
mod generics;
mod iteration;
mod members;
mod mutation;
mod operators;
mod primitives;
mod program;
mod promises;
mod regex;
mod typed_arrays;

pub use bindings::infer_bindings;
pub use expressions::{check_expressions, ClassFacts, Facts, Typer};
pub use functions::{Passed, Site};
pub use program::{infer_program, ModuleTypes};
