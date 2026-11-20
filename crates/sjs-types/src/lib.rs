//! Type representation and normalization (SPEC.md §3.1, §3.2, §11.1; STORIES.md US-080).
//!
//! Types are immutable and interned in a [`Types`]: [`Types::intern`] puts every type in
//! its one canonical form, so two types are equal exactly when their [`TypeId`]s are.
//! A union is a normalized set: flat, without duplicates, without a member another
//! member already holds, in an order that depends on the types alone, and never of
//! fewer than two members.

mod literal;

pub use literal::{Integer, Literal, Number};
use std::cmp::Ordering;
use std::collections::HashMap;

/// An interned type: equal ids are equal types, within one [`Types`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(u32);

impl TypeId {
    /// `number`: an IEEE-754 double.
    pub const NUMBER: Self = Self(0);
    /// `int32`: a `number` that is an integer in [-2³¹, 2³¹).
    pub const INT32: Self = Self(1);
    /// `bigint`.
    pub const BIGINT: Self = Self(2);
    /// `string`.
    pub const STRING: Self = Self(3);
    /// `boolean`.
    pub const BOOLEAN: Self = Self(4);
    /// `undefined`: the absent value.
    pub const UNDEFINED: Self = Self(5);
    /// `null`: the host's, never the program's own.
    pub const NULL: Self = Self(6);
    /// `unknown`: any value at all.
    pub const UNKNOWN: Self = Self(7);
    /// `never`: no value.
    pub const NEVER: Self = Self(8);
}

/// A class: classes are nominal, told apart by this alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub u32);

/// A type variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarId(pub u32);

/// A key of an object shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Property {
    /// The key.
    pub key: String,
    /// The type of its value.
    pub ty: TypeId,
    /// Whether the value's literal type is retained (SPEC.md §3.2: a property set
    /// from a literal).
    pub literal: bool,
}

/// A parameter of a function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Param {
    /// Its type.
    pub ty: TypeId,
    /// Whether an argument may be left out, which is not the same as passing
    /// `undefined` to a parameter of type `T | undefined`.
    pub optional: bool,
}

/// A function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Function {
    /// The type parameters it is generalized over.
    pub type_params: Vec<VarId>,
    /// The parameters, in order.
    pub params: Vec<Param>,
    /// The element type of the rest parameter, when there is one.
    pub rest: Option<TypeId>,
    /// What it returns.
    pub returns: TypeId,
}

/// The generic collections of the ambient library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Builtin {
    /// `Map<K, V>`.
    Map,
    /// `Set<T>`.
    Set,
    /// `WeakMap<K, V>`.
    WeakMap,
}

/// A type. Nested types are [`TypeId`]s of the same [`Types`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// `number`.
    Number,
    /// `int32`.
    Int32,
    /// `bigint`.
    BigInt,
    /// `string`.
    String,
    /// `boolean`.
    Boolean,
    /// A literal type: one value of a primitive.
    Literal(Literal),
    /// An object shape: a fixed set of keys. Interned with its keys in order, so the
    /// order they are given in does not matter; of a key given twice the last stays.
    Object(Vec<Property>),
    /// A dictionary (SPEC.md §4.6): `string` keys, one value type.
    Dictionary(TypeId),
    /// An array.
    Array {
        /// The type of its elements.
        element: TypeId,
        /// Whether the program mutates it anywhere (SPEC.md §3.4).
        mutated: bool,
    },
    /// A tuple: an array of a fixed length, each element with its type.
    Tuple(Vec<TypeId>),
    /// A function.
    Function(Function),
    /// An instance of a class.
    Class(ClassId),
    /// A type variable.
    Var(VarId),
    /// `Promise<T>`.
    Promise(TypeId),
    /// An iterator over values of a type.
    Iterator {
        /// What it yields.
        item: TypeId,
        /// Whether it is an async iterator.
        asynchronous: bool,
    },
    /// A generator.
    Generator {
        /// What it yields.
        yields: TypeId,
        /// What it returns when done.
        returns: TypeId,
        /// Whether it is an async generator.
        asynchronous: bool,
    },
    /// A generic collection with its type arguments.
    Builtin(Builtin, Vec<TypeId>),
    /// A union. Interned through [`Types::union`]: what [`Types::get`] returns is
    /// always normalized, with two members or more.
    Union(Vec<TypeId>),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// `unknown`.
    Unknown,
    /// `never`.
    Never,
}

/// A datum of a type that is no type: what tells two types of one kind apart before
/// the types inside them are compared.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Atom<'a> {
    Count(usize),
    Flag(bool),
    Id(u32),
    Key(&'a str),
    Literal(&'a Literal),
}

/// The interner: every type of a program, each once.
#[derive(Debug, Clone)]
pub struct Types {
    types: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl Default for Types {
    fn default() -> Self {
        Self::new()
    }
}

impl Types {
    /// An interner holding the primitives, at the ids [`TypeId`] names.
    #[must_use]
    pub fn new() -> Self {
        let mut types = Self {
            types: Vec::new(),
            ids: HashMap::new(),
        };
        let primitives = [
            Type::Number,
            Type::Int32,
            Type::BigInt,
            Type::String,
            Type::Boolean,
            Type::Undefined,
            Type::Null,
            Type::Unknown,
            Type::Never,
        ];
        for primitive in primitives {
            types.store(primitive);
        }
        types
    }

    /// How many types are interned: never none, for the primitives always are.
    #[must_use]
    pub fn count(&self) -> usize {
        self.types.len()
    }

    /// The type an id stands for.
    ///
    /// # Panics
    ///
    /// When the id is of another interner that holds more types than this one.
    #[must_use]
    pub fn get(&self, id: TypeId) -> &Type {
        &self.types[id.0 as usize]
    }

    /// The id of a type in canonical form, which is new only if no equal type was
    /// interned before.
    fn store(&mut self, ty: Type) -> TypeId {
        if let Some(&id) = self.ids.get(&ty) {
            return id;
        }
        let id = TypeId(u32::try_from(self.types.len()).unwrap_or(u32::MAX));
        self.types.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }

    /// Interns a type: one id per type, however it is written. The keys of a shape
    /// are put in order and a union is normalized ([`Types::union`]).
    pub fn intern(&mut self, ty: Type) -> TypeId {
        match ty {
            Type::Union(members) => self.union(members),
            Type::Object(mut properties) => {
                // Stable, so of two properties of one key the later is the last.
                properties.sort_by(|a, b| a.key.cmp(&b.key));
                let mut kept: Vec<Property> = Vec::new();
                for property in properties {
                    if kept.last().is_some_and(|last| last.key == property.key) {
                        kept.pop();
                    }
                    kept.push(property);
                }
                self.store(Type::Object(kept))
            }
            other => self.store(other),
        }
    }

    /// The literal type of a value.
    pub fn literal(&mut self, literal: Literal) -> TypeId {
        self.store(Type::Literal(literal))
    }

    /// The union of types, normalized: a union among them is taken apart; `never` and
    /// a type given twice are dropped; `unknown` is all there is when it is there; a
    /// literal is dropped when its primitive is there, `int32` when `number` is, and
    /// `true | false` is `boolean`; what is left is put in the order of
    /// [`Types::compare`]. No member is `never`, one member is itself.
    pub fn union(&mut self, members: impl IntoIterator<Item = TypeId>) -> TypeId {
        let mut flat: Vec<TypeId> = Vec::new();
        let mut pending: Vec<TypeId> = members.into_iter().collect();
        pending.reverse();
        while let Some(member) = pending.pop() {
            match self.get(member) {
                Type::Union(inner) => pending.extend(inner.iter().rev()),
                _ => flat.push(member),
            }
        }
        if flat.contains(&TypeId::UNKNOWN) {
            return TypeId::UNKNOWN;
        }
        let truth = [true, false].map(|value| self.literal(Literal::Boolean(value)));
        if truth.iter().all(|value| flat.contains(value)) {
            flat.push(TypeId::BOOLEAN);
        }
        let has = |id: TypeId| flat.contains(&id);
        let absorbed = |id: TypeId| match self.get(id) {
            Type::Never => true,
            Type::Int32 => has(TypeId::NUMBER),
            Type::Literal(literal) => {
                let primitive = literal.primitive();
                has(primitive) || (primitive == TypeId::INT32 && has(TypeId::NUMBER))
            }
            _ => false,
        };
        let mut kept: Vec<TypeId> = flat.iter().copied().filter(|&id| !absorbed(id)).collect();
        kept.sort_by(|&a, &b| self.compare(a, b));
        kept.dedup();
        match kept.as_slice() {
            [] => TypeId::NEVER,
            [one] => *one,
            _ => self.store(Type::Union(kept)),
        }
    }

    /// What a type is made of: its kind, what it holds that is no type, and the types
    /// inside it.
    fn parts(&self, id: TypeId) -> (u8, Vec<Atom<'_>>, Vec<TypeId>) {
        match self.get(id) {
            Type::Number => (0, Vec::new(), Vec::new()),
            Type::Int32 => (1, Vec::new(), Vec::new()),
            Type::BigInt => (2, Vec::new(), Vec::new()),
            Type::String => (3, Vec::new(), Vec::new()),
            Type::Boolean => (4, Vec::new(), Vec::new()),
            Type::Literal(literal) => (5, vec![Atom::Literal(literal)], Vec::new()),
            Type::Object(properties) => {
                let mut atoms = vec![Atom::Count(properties.len())];
                for property in properties {
                    atoms.push(Atom::Key(&property.key));
                    atoms.push(Atom::Flag(property.literal));
                }
                (6, atoms, properties.iter().map(|p| p.ty).collect())
            }
            Type::Dictionary(value) => (7, Vec::new(), vec![*value]),
            Type::Array { element, mutated } => (8, vec![Atom::Flag(*mutated)], vec![*element]),
            Type::Tuple(elements) => (9, vec![Atom::Count(elements.len())], elements.clone()),
            Type::Function(function) => {
                let mut atoms = vec![Atom::Count(function.type_params.len())];
                atoms.extend(function.type_params.iter().map(|var| Atom::Id(var.0)));
                atoms.push(Atom::Count(function.params.len()));
                atoms.extend(function.params.iter().map(|p| Atom::Flag(p.optional)));
                atoms.push(Atom::Flag(function.rest.is_some()));
                let mut inside: Vec<TypeId> = function.params.iter().map(|p| p.ty).collect();
                inside.extend(function.rest);
                inside.push(function.returns);
                (10, atoms, inside)
            }
            Type::Class(class) => (11, vec![Atom::Id(class.0)], Vec::new()),
            Type::Var(var) => (12, vec![Atom::Id(var.0)], Vec::new()),
            Type::Promise(value) => (13, Vec::new(), vec![*value]),
            Type::Iterator { item, asynchronous } => {
                (14, vec![Atom::Flag(*asynchronous)], vec![*item])
            }
            Type::Generator {
                yields,
                returns,
                asynchronous,
            } => (15, vec![Atom::Flag(*asynchronous)], vec![*yields, *returns]),
            Type::Builtin(builtin, arguments) => {
                let atoms = vec![Atom::Id(*builtin as u32), Atom::Count(arguments.len())];
                (16, atoms, arguments.clone())
            }
            Type::Union(members) => (17, vec![Atom::Count(members.len())], members.clone()),
            Type::Null => (18, Vec::new(), Vec::new()),
            Type::Undefined => (19, Vec::new(), Vec::new()),
            Type::Unknown => (20, Vec::new(), Vec::new()),
            Type::Never => (21, Vec::new(), Vec::new()),
        }
    }

    /// The canonical order of types, which the members of a union are kept in: by
    /// kind (the primitives, literals by value, shapes, dictionaries, arrays, tuples,
    /// functions, classes, variables, promises, iterators, generators, collections,
    /// then `null` and `undefined` last), then by what the types hold. It depends on
    /// the types alone, not on when they were interned.
    #[must_use]
    pub fn compare(&self, a: TypeId, b: TypeId) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        let (kind, atoms, inside) = self.parts(a);
        let (other_kind, other_atoms, other_inside) = self.parts(b);
        let shallow = kind.cmp(&other_kind).then_with(|| atoms.cmp(&other_atoms));
        let pairs = inside.iter().zip(&other_inside);
        pairs.fold(shallow, |order, (&x, &y)| {
            order.then_with(|| self.compare(x, y))
        })
    }
}
