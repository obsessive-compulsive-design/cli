//! Typed arrays, `ArrayBuffer` and `DataView` (SPEC.md §6.1). Each is a class of the
//! ambient library, with a class id of its own that no class of a program has: its
//! fields and methods are told to [`Facts`] as those of a class are, so an instance
//! is read, called, iterated and compared as any class instance is. A typed array is
//! indexed as an array is: an element, or `undefined`.

use crate::ambient::{add_named_classes, intl, NAMED};
use crate::classes::UNKNOWN_BASE;
use crate::expressions::{ClassFacts, Facts};
use crate::platform::{add_platform, platform_classes};
use sjs_types::{ClassId, Function, Param, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

/// The first class id of the ambient library: the ids from here on are its own.
pub(crate) const AMBIENT_CLASSES: u32 = 0xFFFF_FE00;

/// The typed arrays, each with whether it holds bigints.
const TYPED_ARRAYS: [(&str, bool); 12] = [
    ("Int8Array", false),
    ("Uint8Array", false),
    ("Uint8ClampedArray", false),
    ("Int16Array", false),
    ("Uint16Array", false),
    ("Int32Array", false),
    ("Uint32Array", false),
    ("Float16Array", false),
    ("Float32Array", false),
    ("Float64Array", false),
    ("BigInt64Array", true),
    ("BigUint64Array", true),
];

/// The other classes of the ambient library that have a type.
const BUFFERS: [&str; 2] = ["ArrayBuffer", "DataView"];

/// What a method gives.
#[derive(Clone, Copy)]
enum Gives {
    Element,
    MaybeElement,
    Itself,
    Boolean,
    Int32,
    Undefined,
    Text,
    Places,
    Elements,
    Entries,
}

/// The methods of a typed array: what each gives, and whether it takes a callback
/// of an element and its place.
const METHODS: [(&str, Gives, bool); 32] = [
    ("at", Gives::MaybeElement, false),
    ("copyWithin", Gives::Itself, false),
    ("entries", Gives::Entries, false),
    ("every", Gives::Boolean, true),
    ("fill", Gives::Itself, false),
    ("filter", Gives::Itself, true),
    ("find", Gives::MaybeElement, true),
    ("findIndex", Gives::Int32, true),
    ("findLast", Gives::MaybeElement, true),
    ("findLastIndex", Gives::Int32, true),
    ("forEach", Gives::Undefined, true),
    ("includes", Gives::Boolean, false),
    ("indexOf", Gives::Int32, false),
    ("join", Gives::Text, false),
    ("keys", Gives::Places, false),
    ("lastIndexOf", Gives::Int32, false),
    ("map", Gives::Itself, true),
    ("reduce", Gives::Element, false),
    ("reduceRight", Gives::Element, false),
    ("reverse", Gives::Itself, false),
    ("set", Gives::Undefined, false),
    ("slice", Gives::Itself, false),
    ("some", Gives::Boolean, true),
    ("sort", Gives::Itself, false),
    ("subarray", Gives::Itself, false),
    ("toLocaleString", Gives::Text, false),
    ("toReversed", Gives::Itself, false),
    ("toSorted", Gives::Itself, false),
    ("toString", Gives::Text, false),
    ("values", Gives::Elements, false),
    ("with", Gives::Itself, false),
    ("[Symbol.iterator]", Gives::Elements, false),
];

/// The class of the ambient library a global names, when it names one with a type.
pub(crate) fn ambient_class(name: &str) -> Option<ClassId> {
    let typed = TYPED_ARRAYS.iter().map(|(typed, _)| *typed);
    let mut names = typed.chain(BUFFERS).chain(NAMED);
    // The constructors of `Intl` come last, in the order of their file.
    let of_intl = name.strip_prefix("Intl.").and_then(|name| {
        let mut tables = intl().iter().filter(|(table, _)| table != "Intl");
        tables.position(|(table, _)| table == name)
    });
    let place = names.position(|known| known == name);
    let place = place.or(of_intl.map(|place| place + 14 + NAMED.len()));
    // Then the interfaces and the namespaces of the platform.
    let before = 14 + NAMED.len() + intl().len() - 1;
    let of_platform = platform_classes().position(|known| known == name);
    let place = place.or(of_platform.map(|place| place + before))?;
    let place = u32::try_from(place).unwrap_or_default();
    Some(ClassId(AMBIENT_CLASSES + 2 * place))
}

/// Whether a class is one of the ambient library.
pub(crate) fn is_ambient(class: ClassId) -> bool {
    (AMBIENT_CLASSES..UNKNOWN_BASE.0).contains(&class.0)
}

/// What a typed array of this class holds; `None` for any other class.
pub(crate) fn element_of(class: ClassId) -> Option<TypeId> {
    let place = class.0.checked_sub(AMBIENT_CLASSES)? / 2;
    let place = usize::try_from(place).unwrap_or(usize::MAX);
    let typed = TYPED_ARRAYS.get(place);
    typed.map(|(_, big)| if *big { TypeId::BIGINT } else { TypeId::NUMBER })
}

/// A function that takes anything and gives a `returns`.
pub(crate) fn taking_any(types: &mut Types, returns: TypeId) -> TypeId {
    types.intern(Type::Function(Function {
        type_params: Vec::new(),
        params: Vec::new(),
        rest: Some(TypeId::UNKNOWN),
        returns,
    }))
}

/// What is known of one class of the ambient library, told to `facts`.
struct Shape<'a> {
    facts: &'a mut Facts,
    types: &'a mut Types,
    class: ClassId,
    instance: TypeId,
    known: ClassFacts,
}

impl Shape<'_> {
    fn field(&mut self, name: &str, ty: TypeId) {
        self.known.declared.insert(name.to_owned());
        let fields = self.facts.fields.entry(self.class).or_default();
        fields.insert(name.to_owned(), ty);
    }

    fn method(&mut self, name: &str, signature: TypeId) {
        self.known.declared.insert(name.to_owned());
        self.known.signatures.insert(name.to_owned(), signature);
        let methods = self.facts.methods.entry(self.class).or_default();
        methods.insert(name.to_owned());
    }

    fn plain(&mut self, name: &str, returns: TypeId) {
        let signature = taking_any(self.types, returns);
        self.method(name, signature);
    }
}

/// Tells `facts` the classes of the ambient library that have a type: the typed
/// arrays, `ArrayBuffer` and `DataView`.
pub(crate) fn add_ambient_classes(facts: &mut Facts, types: &mut Types) {
    add_named_classes(facts, types);
    add_platform(facts, types);
    let arrays = TYPED_ARRAYS.iter().map(|(array, _)| *array);
    let buffer = types.intern(Type::Class(ClassId(AMBIENT_CLASSES + 24), Vec::new()));
    for name in arrays.chain(BUFFERS) {
        let class = ambient_class(name).unwrap_or(ClassId(AMBIENT_CLASSES));
        let instance = types.intern(Type::Class(class, Vec::new()));
        facts.class_names.insert(class, name.to_owned());
        facts.fields.insert(class, BTreeMap::new());
        facts.methods.insert(class, BTreeSet::new());
        let made = taking_any(types, instance);
        let known = ClassFacts {
            constructor: Some(made),
            ..ClassFacts::default()
        };
        let mut shape = Shape {
            facts: &mut *facts,
            types: &mut *types,
            class,
            instance,
            known,
        };
        match (name, element_of(class)) {
            (_, Some(element)) => typed_array(&mut shape, element, buffer),
            ("ArrayBuffer", _) => {
                for field in ["byteLength", "maxByteLength"] {
                    shape.field(field, TypeId::INT32);
                }
                for field in ["resizable", "detached"] {
                    shape.field(field, TypeId::BOOLEAN);
                }
                for method in ["slice", "transfer", "transferToFixedLength"] {
                    shape.plain(method, instance);
                }
                shape.plain("resize", TypeId::UNDEFINED);
            }
            _ => {
                shape.field("buffer", buffer);
                for field in ["byteLength", "byteOffset"] {
                    shape.field(field, TypeId::INT32);
                }
                let numbers = [
                    "Int8", "Uint8", "Int16", "Uint16", "Int32", "Uint32", "Float16", "Float32",
                    "Float64",
                ];
                for (kinds, held) in [
                    (&numbers[..], TypeId::NUMBER),
                    (&["BigInt64", "BigUint64"][..], TypeId::BIGINT),
                ] {
                    for kind in kinds {
                        shape.plain(&format!("get{kind}"), held);
                        shape.plain(&format!("set{kind}"), TypeId::UNDEFINED);
                    }
                }
            }
        }
        let Shape { known, .. } = shape;
        facts.classes.insert(class, known);
    }
}

/// The fields and methods of a typed array of `element`s.
fn typed_array(shape: &mut Shape<'_>, element: TypeId, buffer: TypeId) {
    shape.field("buffer", buffer);
    for field in ["length", "byteLength", "byteOffset"] {
        shape.field(field, TypeId::INT32);
    }
    let maybe = shape.types.union([element, TypeId::UNDEFINED]);
    let iterator = |types: &mut Types, item: TypeId| {
        types.intern(Type::Iterator {
            item,
            asynchronous: false,
        })
    };
    let entry = shape
        .types
        .intern(Type::Tuple(vec![TypeId::INT32, element]));
    // A callback of an element and its place, which may give anything.
    let visitor = shape.types.intern(Type::Function(Function {
        type_params: Vec::new(),
        params: vec![element, TypeId::INT32]
            .into_iter()
            .map(|ty| Param {
                ty,
                optional: false,
            })
            .collect(),
        rest: None,
        returns: TypeId::UNKNOWN,
    }));
    for (name, gives, visits) in METHODS {
        let returns = match gives {
            Gives::Element => element,
            Gives::MaybeElement => maybe,
            Gives::Itself => shape.instance,
            Gives::Boolean => TypeId::BOOLEAN,
            Gives::Int32 => TypeId::INT32,
            Gives::Undefined => TypeId::UNDEFINED,
            Gives::Text => TypeId::STRING,
            Gives::Places => iterator(shape.types, TypeId::INT32),
            Gives::Elements => iterator(shape.types, element),
            Gives::Entries => iterator(shape.types, entry),
        };
        let params = if visits {
            vec![Param {
                ty: visitor,
                optional: false,
            }]
        } else {
            Vec::new()
        };
        let signature = shape.types.intern(Type::Function(Function {
            type_params: Vec::new(),
            params,
            rest: Some(TypeId::UNKNOWN),
            returns,
        }));
        shape.method(name, signature);
    }
}

#[cfg(test)]
mod tests {
    use super::{ambient_class, is_ambient, AMBIENT_CLASSES, BUFFERS, TYPED_ARRAYS};
    use crate::ambient::{intl, NAMED};
    use crate::platform::platform_classes;

    #[test]
    fn every_class_of_the_ambient_library_has_an_id_of_its_own() {
        let typed = TYPED_ARRAYS.iter().map(|(typed, _)| (*typed).to_owned());
        let others = BUFFERS.iter().chain(&NAMED).map(|name| (*name).to_owned());
        let of_intl = intl().iter().filter(|(table, _)| table != "Intl");
        let of_intl = of_intl.map(|(table, _)| format!("Intl.{table}"));
        let of_platform = platform_classes().map(str::to_owned);
        let names: Vec<String> = typed
            .chain(others)
            .chain(of_intl)
            .chain(of_platform)
            .collect();
        assert!(
            names.len() > 60,
            "typed arrays, buffers, errors, `Intl` and the platform"
        );
        for (at, name) in names.iter().enumerate() {
            // Each two apart, for the static side of the one before.
            let expected = AMBIENT_CLASSES + 2 * u32::try_from(at).expect("few");
            let class = ambient_class(name).expect(name);
            assert_eq!(class.0, expected, "{name}");
            assert!(is_ambient(class), "{name}");
        }
        assert_eq!(ambient_class("Nothing"), None);
        assert_eq!(ambient_class("Intl.Nothing"), None);
        assert_eq!(
            ambient_class("Intl.Intl"),
            None,
            "the functions of `Intl` are no class"
        );
    }
}
