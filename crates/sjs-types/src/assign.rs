//! Assignability (SPEC.md §3.3): whether every value of one type may go where another
//! type is expected. Structural for shapes, arrays, tuples and functions, nominal for
//! classes. E101 (an excess key in a literal) is not decided here: it needs the call
//! site.

use crate::{ClassId, Function, Type, TypeId, Types};
use std::collections::BTreeSet;

/// Judges assignability among the types of a [`Types`], given what each class extends.
pub struct Assignability<'a> {
    types: &'a Types,
    superclass: &'a dyn Fn(ClassId) -> Option<ClassId>,
}

impl<'a> Assignability<'a> {
    /// A judge over `types`; `superclass` is the class a class extends, if any.
    #[must_use]
    pub fn new(types: &'a Types, superclass: &'a dyn Fn(ClassId) -> Option<ClassId>) -> Self {
        Self { types, superclass }
    }

    /// Whether a value of type `from` may go where `to` is expected.
    ///
    /// - A type is assignable to itself; `never` to everything; everything to
    ///   `unknown`, and `unknown` to nothing else.
    /// - A union is assignable when every member is; to a union, when some member
    ///   takes it.
    /// - `int32` to `number`; a literal to its primitive; `null` and `undefined` only
    ///   to themselves.
    /// - A shape to a shape with no key it lacks, each key's type assignable (width).
    /// - An array the program never mutates to another such, by its elements; a
    ///   mutated array only to itself. A tuple to a tuple of its length, element by
    ///   element, and to an unmutated array that takes every element.
    /// - A function to one it can be called as: it takes what the other is given
    ///   (parameters are contravariant; one that may be left out must be optional;
    ///   fewer parameters do for more, but none is hidden: a parameter or a rest the
    ///   other never passes is refused, optional or not, or assignability would not
    ///   be transitive) and returns what the other returns.
    /// - A class to itself and to the classes it extends.
    /// - A promise, an iterator, a generator and a dictionary by what they hold; a
    ///   generator is an iterator of what it yields. `Map`, `Set` and `WeakMap` are
    ///   mutable, so only to themselves, like a type variable.
    #[must_use]
    pub fn assignable(&self, from: TypeId, to: TypeId) -> bool {
        if from == to || to == TypeId::UNKNOWN || from == TypeId::NEVER {
            return true;
        }
        match (self.types.get(from), self.types.get(to)) {
            (Type::Union(members), _) => members.iter().all(|&member| self.assignable(member, to)),
            (_, Type::Union(members)) => {
                members.iter().any(|&member| self.assignable(from, member))
            }
            (Type::Int32, Type::Number) => true,
            (Type::Literal(literal), _) => self.assignable(literal.primitive(), to),
            (Type::Object(given), Type::Object(wanted)) => wanted.iter().all(|key| {
                let held = given.iter().find(|held| held.key == key.key);
                held.is_some_and(|held| self.assignable(held.ty, key.ty))
            }),
            (
                Type::Array {
                    element: given,
                    mutated: false,
                },
                Type::Array {
                    element: wanted,
                    mutated: false,
                },
            )
            | (Type::Dictionary(given), Type::Dictionary(wanted))
            | (Type::Promise(given), Type::Promise(wanted)) => self.assignable(*given, *wanted),
            (Type::Tuple(given), Type::Tuple(wanted)) => {
                let pairs = given.iter().zip(wanted);
                given.len() == wanted.len()
                    && pairs
                        .into_iter()
                        .all(|(&held, &slot)| self.assignable(held, slot))
            }
            (
                Type::Tuple(given),
                Type::Array {
                    element,
                    mutated: false,
                },
            ) => given.iter().all(|&held| self.assignable(held, *element)),
            (Type::Function(given), Type::Function(wanted)) => self.callable_as(given, wanted),
            (Type::Class(given), Type::Class(wanted)) => self.extends(*given, *wanted),
            (
                Type::Iterator {
                    item: given,
                    asynchronous,
                }
                | Type::Generator {
                    yields: given,
                    asynchronous,
                    ..
                },
                Type::Iterator {
                    item: wanted,
                    asynchronous: other,
                },
            ) => asynchronous == other && self.assignable(*given, *wanted),
            (
                Type::Generator {
                    yields,
                    returns,
                    asynchronous,
                },
                Type::Generator {
                    yields: wanted_yields,
                    returns: wanted_returns,
                    asynchronous: other,
                },
            ) => {
                asynchronous == other
                    && self.assignable(*yields, *wanted_yields)
                    && self.assignable(*returns, *wanted_returns)
            }
            _ => false,
        }
    }

    /// Whether the class `given` is `wanted` or extends it, however far up.
    fn extends(&self, given: ClassId, wanted: ClassId) -> bool {
        let mut seen = BTreeSet::new();
        let mut at = Some(given);
        while let Some(class) = at.filter(|class| seen.insert(*class)) {
            if class == wanted {
                return true;
            }
            at = (self.superclass)(class);
        }
        false
    }

    /// Whether `given` may be called wherever `wanted` is: with the arguments `wanted`
    /// is called with, for the result `wanted` gives.
    fn callable_as(&self, given: &Function, wanted: &Function) -> bool {
        // What `given` takes at a place: the parameter, whether it may be left out;
        // nothing at all past its parameters when it has no rest, which ignores.
        let slot = |at: usize| match given.params.get(at) {
            Some(param) => Some((param.ty, param.optional)),
            None => given.rest.map(|rest| (rest, true)),
        };
        let places = given.params.len().max(wanted.params.len());
        let fits = (0..places).all(|at| match (wanted.params.get(at), slot(at)) {
            // An argument `given` ignores.
            (_, None) => true,
            // Passed: `given` must take it, and when it may be left out, allow that.
            (Some(passed), Some((takes, optional))) => {
                self.assignable(passed.ty, takes) && (optional || !passed.optional)
            }
            // Past what `wanted` is given by name there are only its rest arguments. A
            // parameter there must take them and may be left out; with no rest it
            // would be a parameter nothing shows, which a function with more
            // parameters could then be given anything for.
            (None, Some((takes, optional))) => {
                optional && wanted.rest.is_some_and(|rest| self.assignable(rest, takes))
            }
        });
        // A rest parameter takes the rest arguments, and for the same reason is not
        // hidden: a function with one goes only where rest arguments are passed.
        let rest = given.rest.is_none_or(|takes| {
            wanted
                .rest
                .is_some_and(|passed| self.assignable(passed, takes))
        });
        given.type_params == wanted.type_params
            && fits
            && rest
            && self.assignable(given.returns, wanted.returns)
    }
}
