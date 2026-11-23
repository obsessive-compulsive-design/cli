//! STORIES.md US-092: classes.

use sjs_bind::{bind, Content, Disk, ModuleGraph};
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, infer_program, Facts};
use sjs_syntax::{parse, SyntaxTree};
use sjs_testkit::fs::TempDir;
use sjs_types::{ClassId, Printer, Type, TypeId, Types};

struct Checked {
    /// `name: type` of each declaration that has one, by name.
    types: Vec<(String, String)>,
    /// The members of each class side that has a type, by the name of the side:
    /// `name: type`, each kind of member apart.
    members: Vec<(String, String)>,
    /// `CODE @line:column text`.
    found: Vec<String>,
}

impl Checked {
    fn of(&self, name: &str) -> String {
        // The last of the name: a binding, after the parameters of the classes.
        let mut types = self.types.iter().rev();
        let found = types.find(|(declared, _)| declared == name);
        found.map_or("?".to_owned(), |(_, ty)| ty.clone())
    }

    fn class(&self, name: &str) -> String {
        let found = self.members.iter().find(|(class, _)| class == name);
        found.map_or("?".to_owned(), |(_, members)| members.clone())
    }
}

/// What checking one inferred file gives.
fn shown(
    source: &SourceFile,
    tree: &SyntaxTree,
    bindings: &sjs_bind::Bindings,
    facts: &Facts,
    types: &mut Types,
) -> Checked {
    let diagnostics = check_expressions(source, tree, bindings, facts, types);
    let name = |class: ClassId| {
        let name = facts.class_names.get(&class);
        name.map_or(format!("Class{}", class.0), Clone::clone)
    };
    let printer = Printer::new(types, &name);
    // An `async` function that never awaits is another story's finding (US-094).
    let found = diagnostics
        .iter()
        .filter(|d| d.code.as_str() != "E118")
        .map(|d| {
            let covered = source.slice(&d.span).expect("in the file");
            format!(
                "{} @{}:{} {covered}",
                d.code.as_str(),
                d.span.start.line,
                d.span.start.column
            )
        })
        .collect();
    let mut signatures = Vec::new();
    for (at, declaration) in bindings.declarations.iter().enumerate() {
        if let Some(&ty) = facts.declarations.get(&at) {
            let shown = if matches!(types.get(ty), Type::Function(_)) {
                printer.signature(&declaration.name, &[], ty)
            } else {
                printer.print(ty)
            };
            signatures.push((declaration.name.clone(), shown));
        }
    }
    let mut members = Vec::new();
    for (class, fields) in &facts.fields {
        let mut parts: Vec<String> = fields
            .iter()
            .map(|(key, ty)| format!("{key}: {}", printer.print(*ty)))
            .collect();
        if let Some(shape) = facts.classes.get(class) {
            let optional = |ty: &Option<_>| ty.map_or("?".to_owned(), |ty| printer.print(ty));
            let signatures = shape.signatures.iter();
            parts.extend(signatures.map(|(key, ty)| printer.signature(key, &[], *ty)));
            let getters = shape.getters.iter();
            parts.extend(getters.map(|(key, ty)| format!("get {key}: {}", optional(ty))));
            let setters = shape.setters.iter();
            parts.extend(setters.map(|(key, ty)| format!("set {key}: {}", optional(ty))));
        }
        members.push((name(*class), parts.join("; ")));
    }
    Checked {
        types: signatures,
        members,
        found,
    }
}

fn checked(text: &str) -> Checked {
    checked_knowing(text, false)
}

/// Infers and checks `text`; when `point` is set, a class `Point` with a field `x` is
/// known already, as class 0, and `p` is an instance of it where it is declared.
fn checked_knowing(text: &str, point: bool) -> Checked {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let mut facts = Facts::default();
    if point {
        let fields = [("x".to_owned(), TypeId::NUMBER)].into_iter().collect();
        facts.fields.insert(ClassId(0), fields);
        facts.class_names.insert(ClassId(0), "Point".to_owned());
        let p = bindings.declarations.iter().position(|d| d.name == "p");
        let instance = types.intern(Type::Class(ClassId(0), Vec::new()));
        facts.declarations.extend(p.map(|p| (p, instance)));
    }
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    shown(&source, &tree, &bindings, &facts, &mut types)
}

fn found(text: &str) -> Vec<String> {
    checked(text).found
}

const NONE: [&str; 0] = [];

/// Infers and checks a program, and gives what each file shows, by its name.
fn program(files: &[(&str, &str)], roots: &[&str]) -> Vec<(String, Checked)> {
    let dir = TempDir::new("us092").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let roots: Vec<_> = roots.iter().map(|root| base.join(root)).collect();
    let (graph, _) = ModuleGraph::load(&Disk, &base, &roots).expect("loads");
    let (links, _) = graph.link();
    let mut types = Types::new();
    let modules = infer_program(&graph, &links, &mut types);
    let mut out = Vec::new();
    for module in graph.modules() {
        let inferred = modules.get(&module.path);
        if let Some((inferred, Content::Script { source, tree, .. })) =
            inferred.map(|inferred| (inferred, &module.content))
        {
            let name = module.path.file_name().expect("a name");
            let checked = shown(
                source,
                tree,
                &inferred.bindings,
                &inferred.facts,
                &mut types,
            );
            out.push((name.to_string_lossy().into_owned(), checked));
        }
    }
    out
}

#[test]
fn fields_are_typed_from_initializers_and_from_what_the_class_assigns() {
    let c = checked("class P {\n  x\n  y = 0\n  #z = undefined\n  constructor(x) {\n    this.x = x\n  }\n  norm() {\n    return this.x * this.y\n  }\n  later() {\n    this.#z = \"a\"\n    return this.#z\n  }\n}\nconst p = new P(2)\nconst n = p.norm()\nconst l = p.later()\nconst y = p.y\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        c.class("P"),
        "#z: string | undefined; x: int32; y: int32; later(): string | undefined; norm(): number",
        "a field given `undefined` and later a string is either"
    );
    assert_eq!(
        c.of("P"),
        "P(a: int32): P",
        "a class as a value is its constructor"
    );
    assert_eq!(
        (c.of("p"), c.of("n"), c.of("y")),
        ("P".into(), "number".into(), "int32".into())
    );
    assert_eq!(c.of("l"), "string | undefined");
    // A field read from itself is typed from what else it is given.
    let c = checked("class A {\n  n = 0\n  self = undefined\n  inc() {\n    this.n = this.n + 1\n    this.self = this\n    return this\n  }\n  async later() {\n    return this.n\n  }\n  *each() {\n    yield 1\n  }\n  [\"k\"]() {}\n}\nconst a = new A().inc().inc()\nconst l = a.later()\nconst e = a.each()\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        c.class("A"),
        "n: number; self: A | undefined; each(): Generator<int32, undefined>; inc(): A; later(): Promise<number>"
    );
    assert_eq!(
        (c.of("a"), c.of("l"), c.of("e")),
        (
            "A".into(),
            "Promise<number>".into(),
            "Generator<int32, undefined>".into()
        )
    );
    // A method read and not called is no call of it.
    let c = checked(
        "class A {\n  m(a) {\n    return a + 1\n  }\n}\nconst a = new A()\na.m(1)\na.m.undefined\n",
    );
    assert_eq!(
        (c.found.clone(), c.class("A")),
        (
            vec!["E138 @8:3 m".to_owned()],
            "m(a: int32): number".to_owned()
        )
    );
    // Only `=` gives a field a type, and only on `this` or on the class by name; a
    // field may be named `static`.
    let c = checked("class A {\n  static count = 0\n  n = 0\n  static = 1\n  m(o) {\n    this.n += 1.5\n    o.count = \"s\"\n    o.n = \"s\"\n  }\n}\nconst s = new A().static\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.class("A"), c.class("typeof A"), c.of("s")),
        (
            "n: int32; static: int32".into(),
            "count: int32".into(),
            "int32".into()
        )
    );
    // A field given nothing that has a type has none, and nothing is said of it.
    let c = checked("class A {\n  v = f()\n  w = 1\n  m() {\n    this.w = g()\n    return this.v\n  }\n}\nconst a = new A()\nconst v = a.v\nconst w = a.w\na.v = 1\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.class("A"), c.of("v"), c.of("w")),
        (String::new(), "?".into(), "?".into())
    );
    // One that grows without end is given up on.
    let c = checked(
        "class A {\n  v = 1\n  m() {\n    this.v = [this.v]\n  }\n}\nconst v = new A().v\n",
    );
    assert_eq!((c.found.clone(), c.of("v")), (Vec::new(), "?".to_owned()));
}

#[test]
fn a_field_without_an_initializer_is_assigned_by_the_constructor() {
    assert_eq!(
        found("class A {\n  a\n  b\n  c\n  d\n  #e\n  static s\n  f = 1\n  constructor(v) {\n    this.a = v\n    if (v) {\n      this.b = 1\n    }\n    this.c += 1\n    other.d = 1\n    this.#e = 2\n    v = this.d\n  }\n  m() {\n    this.d = 1\n  }\n}\nnew A(1)\n"),
        ["E114 @3:3 b", "E114 @4:3 c", "E114 @5:3 d"],
        "in a branch, by `+=`, in a method: not assigned straight down the constructor"
    );
    assert_eq!(
        found("class A {\n  a\n  m() {\n    this.a = 1\n  }\n}\n"),
        ["E114 @2:3 a"],
        "no constructor"
    );
    assert_eq!(
        found("class A {\n  a\n  b = undefined\n  constructor() {\n    this.a = 1\n  }\n}\nclass B extends A {\n  c\n  constructor() {\n    super()\n    this.c = 2\n  }\n}\n"),
        NONE
    );
}

#[test]
fn a_key_a_class_does_not_declare_is_not_read_or_written() {
    assert_eq!(
        found("class A {\n  x = 1\n  m() {\n    this.q = 1\n    return this.r\n  }\n}\nconst a = new A()\na.w = 1\na.x = \"s\"\na.x = 2.5\na.m = 1\nconst f = a.m\n"),
        ["E129 @4:10 q", "E137 @5:17 r", "E129 @9:3 w", "E138 @10:7 \"s\"", "E138 @13:13 m"]
    );
    // What a class extends that is no class of the program is not known.
    let c = checked("class C extends EventTarget {\n  m() {\n    this.code = 1\n    return this.message\n  }\n}\nclass D extends ns.E {}\nconst m = new C().m()\nconst d = new D().x\nconst u = new Unknown()\n");
    assert_eq!(
        (c.found.clone(), c.of("m"), c.of("d"), c.of("u")),
        (Vec::new(), "?".into(), "?".into(), "?".into())
    );
    // A private name is a key like another, for the class that has it.
    assert_eq!(
        found("const o = { f: 1 }\nconst w = o.#x\n"),
        ["E137 @2:13 #x"]
    );
}

#[test]
fn accessors_are_read_by_their_getter_and_written_by_their_setter() {
    let c = checked("class S {\n  v\n  constructor(v) {\n    this.v = v * 2\n  }\n  get double() {\n    return this.v * 2\n  }\n  set double(d) {\n    this.v = d / 2\n  }\n  set only(v) {\n    this.v = v\n  }\n}\nconst s = new S(1)\ns.double = 3\ns.double += 1.5\nconst dd = s.double\nconst { v: sv, double } = s\nconst o = s.only\ns.only = 4\ns[\"double\"] = 5\nconst i = s[\"v\"]\n");
    assert_eq!(
        c.found, NONE,
        "a setter that takes an `int32` takes any number"
    );
    assert_eq!(
        c.class("S"),
        "v: number; get double: number; set double: int32; set only: int32"
    );
    assert_eq!(
        (c.of("dd"), c.of("double"), c.of("o"), c.of("i")),
        (
            "number".into(),
            "number".into(),
            "?".into(),
            "number".into()
        )
    );
    assert_eq!(c.of("sv"), "number");
    let c = checked("class T {\n  #c = 0\n  get f() {\n    return this.#c\n  }\n  set f(v) {\n    this.#c = v\n  }\n  get ro() {\n    return 1\n  }\n  get s() {\n    return \"a\"\n  }\n  set s(v) {}\n  get u() {\n    return g()\n  }\n  set u(v) {}\n  set w(v) {}\n}\nconst t = new T()\nt.f = 2\nt.ro = 3\nt.ro += 1\nt.s = 1\nt.u = 1\nt.w = \"x\"\nconst f = t.f\nconst u = t.u\n");
    assert_eq!(
        c.found,
        ["E138 @15:7 s", "E138 @24:3 ro", "E138 @25:3 ro"],
        "a setter takes what its getter gives; a getter alone is not written"
    );
    assert_eq!(
        c.class("T"),
        "#c: int32; get f: int32; get ro: int32; get s: string; get u: ?; set f: int32; set s: int32; set u: int32; set w: string"
    );
    assert_eq!((c.of("f"), c.of("u")), ("int32".into(), "?".into()));
    // A setter nothing is assigned through takes nothing known, and is not judged.
    let c = checked("class T {\n  get s() {\n    return \"a\"\n  }\n  set s(v) {}\n}\nconst t = new T()\nt.s = t.nope\n");
    assert_eq!(
        (c.found.clone(), c.class("T")),
        (
            vec!["E137 @8:9 nope".to_owned()],
            "get s: string; set s: ?".to_owned()
        )
    );
}

#[test]
fn static_members_are_members_of_the_class_itself() {
    let c = checked("class A {\n  static n\n  static list = undefined\n  static {\n    this.n = 1\n    A.list = \"a\"\n  }\n  static bump() {\n    A.n = 2.5\n    return this.n\n  }\n  x = 1\n  other() {\n    A.x = 1\n    return this.n\n  }\n}\nconst n = A.n\nconst l = A.list\nconst b = A.bump()\nA.n = \"s\"\nA.bump = 1\nconst m = A.bump\nconst d = A.nope\nconst x = new A().x\n");
    assert_eq!(
        c.found,
        [
            "E129 @14:7 x",
            "E137 @15:17 n",
            "E138 @21:7 \"s\"",
            "E138 @23:13 bump",
            "E137 @24:13 nope"
        ]
    );
    assert_eq!(c.class("A"), "x: int32");
    assert_eq!(
        c.class("typeof A"),
        "list: string | undefined; n: number | undefined; bump(): number | undefined"
    );
    assert_eq!(
        (c.of("n"), c.of("l"), c.of("b"), c.of("x")),
        (
            "number | undefined".into(),
            "string | undefined".into(),
            "number | undefined".into(),
            "int32".into()
        )
    );
    // The static side of a class extends that of the class it extends.
    let c = checked("class A {\n  static make() {\n    return 1\n  }\n}\nclass B extends A {\n  static more() {\n    return super.make() + this.make()\n  }\n}\nconst m = B.make()\nconst n = B.more()\n");
    assert_eq!(
        (c.found.clone(), c.of("m"), c.of("n")),
        (Vec::new(), "int32".into(), "number".into())
    );
}

#[test]
fn a_class_extends_a_class_and_overrides_what_goes_where_the_overridden_does() {
    let c = checked("class A {\n  static count = 0\n  m(a) {\n    return a + 1\n  }\n  toString() {\n    return \"A\"\n  }\n}\nclass B extends A {\n  m(a) {\n    return \"x\"\n  }\n  #p() {\n    return 1\n  }\n  q() {\n    return this.#p() + super.m(1)\n  }\n}\nconst f = () => 1\nclass C extends f {}\nfunction g() {}\nclass D extends g {}\nconst h = unknown()\nclass E extends h {}\nconst b = new B()\nconst r = b.m(2)\nconst s = new A().m(3)\nconst t = `${b}${new A()}`\nconst q = b.q()\nconst o = { ...b }\nclass N {}\nconst bad = `${new N()}`\nclass Q {\n  toString() {\n    return 1\n  }\n}\nconst worse = `${new Q()}`\n");
    assert_eq!(
        c.found,
        [
            "E138 @11:3 m",
            "E138 @22:17 f",
            "E138 @24:17 g",
            "E125 @34:16 new N()",
            "E125 @40:18 new Q()"
        ],
        "an override that returns a string where a number is returned; what is extended is a class"
    );
    assert_eq!(
        (c.of("r"), c.of("s"), c.of("t"), c.of("q"), c.of("o")),
        (
            "string".into(),
            "number".into(),
            "string".into(),
            "number".into(),
            "{}".into()
        )
    );
    // Fewer parameters do for more, more do not; type parameters are alike by place.
    let c = checked("class A {\n  m(a, b = 1) {\n    return a\n  }\n}\nclass B extends A {\n  m(a) {\n    return a\n  }\n  n(x) {}\n}\nclass C extends B {\n  m(a, b, c) {\n    return a\n  }\n  static m() {}\n}\nconst x = new C().m(1, 2, 3)\nconst y = new A().m(1)\nnew B().m(1, 2)\n");
    assert_eq!(c.found, ["E138 @13:3 m", "E136 @20:1 new B().m(1, 2)"]);
    assert_eq!((c.of("x"), c.of("y")), ("int32".into(), "int32".into()));
    let c = checked("class Box {\n  v\n  constructor(v) {\n    this.v = v\n  }\n  same(x) {\n    return x\n  }\n  two(x, y) {\n    return x\n  }\n}\nclass Int extends Box {\n  constructor() {\n    super(1)\n  }\n  same(x) {\n    return x\n  }\n  two(x) {\n    return 1\n  }\n}\nconst v = new Int().v\n");
    assert_eq!(c.found, ["E138 @20:3 two"]);
    assert_eq!(
        c.class("Box"),
        "v: T; same<U>(a: U): U; two<U, V>(a: U, b: V): U"
    );
    assert_eq!(c.class("Int"), "same<T>(a: T): T; two<T>(a: T): int32");
    assert_eq!(c.of("v"), "int32");
}

#[test]
fn a_class_is_generic_in_what_its_constructor_stores_opaquely() {
    let c = checked("class Box {\n  v\n  constructor(v) {\n    this.v = v\n  }\n  get() {\n    return this.v\n  }\n}\nconst b = new Box(1)\nconst n = b.get()\nconst m = b.v\nconst K = Box\nconst s = { ...b }\nb.v = \"s\"\nBox(1)\nnew Box()\nconst t = new Box(\"a\")\nconst { v: held } = t\n");
    assert_eq!(
        c.found,
        ["E138 @15:7 \"s\"", "E139 @16:1 Box", "E136 @17:1 new Box()"]
    );
    assert_eq!(c.of("Box"), "Box<T>(a: T): Box<T>");
    assert_eq!(c.class("Box"), "v: T; get(): T");
    assert_eq!(
        (c.of("b"), c.of("n"), c.of("m")),
        ("Box<int32>".into(), "int32".into(), "int32".into())
    );
    assert_eq!(
        (c.of("K"), c.of("s")),
        ("K<T>(a: T): Box<T>".into(), "{ v: int32 }".into())
    );
    assert_eq!(
        (c.of("t"), c.of("held")),
        ("Box<string>".into(), "string".into())
    );
    let c = checked("class Pair {\n  a\n  b\n  constructor(a, b) {\n    this.a = a\n    this.b = b\n  }\n  swap() {\n    return new Pair(this.b, this.a)\n  }\n  has(x) {\n    return x === this.a\n  }\n  first(xs) {\n    return xs[0]\n  }\n}\nconst p = new Pair(1, \"s\")\nconst q = p.swap()\nconst h = p.has(2)\nconst f = p.first([true])\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        c.class("Pair"),
        "a: T; b: U; first<V>(a: readonly V[]): V | undefined; has<V>(a: V): boolean; swap(): Pair<U, T>"
    );
    assert_eq!(
        (c.of("p"), c.of("q")),
        ("Pair<int32, string>".into(), "Pair<string, int32>".into())
    );
    assert_eq!(
        (c.of("h"), c.of("f")),
        ("boolean".into(), "boolean | undefined".into())
    );
    // How the class uses the field says what the parameter is; a parameter no
    // member holds is no type parameter of the class; a method that stores is not
    // generic in what it stores.
    let c = checked("class A {\n  v\n  n\n  constructor(v, tag, n) {\n    this.v = v\n    this.n = n\n  }\n  set(v) {\n    this.v = v\n  }\n  twice() {\n    return this.n * 2\n  }\n  #secret(k) {\n    return k\n  }\n  #unused(k) {}\n  go() {\n    return this.#secret(1)\n  }\n  static s(k) {\n    this.v = k\n  }\n}\nconst a = new A(1, \"t\", 2)\na.set(\"s\")\nconst v = a.v\nconst g = a.go()\n");
    assert_eq!(c.found, ["E112 @17:11 k", "E129 @22:10 v"]);
    assert_eq!(c.of("A"), "A<T, U>(a: T, b: U, c: int32): A<T>");
    assert_eq!(
        c.class("A"),
        "n: int32; v: string | T; #secret<U>(a: U): U; go(): int32; set(a: string): undefined; twice(): number"
    );
    assert_eq!(
        (c.of("a"), c.of("v"), c.of("g")),
        ("A<int32>".into(), "int32 | string".into(), "int32".into())
    );
}

#[test]
fn a_class_that_extends_a_generic_one_says_what_its_type_parameters_are() {
    let c = checked("class G {\n  v\n  constructor(v) {\n    this.v = v\n  }\n}\nclass H extends G {\n  constructor(n) {\n    super(n + 1)\n  }\n}\nclass I extends G {\n  constructor(q) {\n    super()\n  }\n  get() {\n    return super.v\n  }\n}\nclass J extends H {}\nclass L extends G {\n  constructor(w) {\n    super(w)\n  }\n  get() {\n    return this.v\n  }\n}\nclass M extends G {}\nconst h = new H(1).v\nconst i = new I(1).v\nconst j = new J(2).v\nconst l = new L(\"s\").get()\nconst m = new M(true).v\nconst make = (K) => new K(1)\nconst k = make(G)\nconst pass = (v) => new H(v)\nconst w = pass(3)\nconst arrow = () => 1\nnew arrow()\nnew make(G)\n");
    assert_eq!(c.found, ["E136 @14:5 super()", "E139 @40:5 arrow"]);
    assert_eq!(
        (c.of("H"), c.of("J")),
        ("H(a: int32): H".into(), "J(a: int32): J".into())
    );
    assert_eq!(
        (c.of("L"), c.of("M")),
        ("L<T>(a: T): L<T>".into(), "M<T>(a: T): M<T>".into())
    );
    assert_eq!(
        (c.of("h"), c.of("i"), c.of("j")),
        ("number".into(), "?".into(), "number".into())
    );
    assert_eq!((c.of("l"), c.of("m")), ("string".into(), "boolean".into()));
    assert_eq!(
        (c.of("k"), c.of("pass"), c.of("w")),
        ("G<int32>".into(), "pass(a: int32): H".into(), "H".into())
    );
    // Only `super(…)` is a call of the constructor extended; an override that only
    // throws goes where a generic method does.
    let c = checked("class A {\n  v\n  constructor(v) {\n    this.v = v * 1\n  }\n  same(x) {\n    return x\n  }\n}\nclass B extends A {\n  constructor() {\n    super(1)\n    log(\"s\")\n  }\n  same() {\n    throw new Error(\"no\")\n  }\n}\n");
    assert_eq!(
        (c.found.clone(), c.of("A")),
        (Vec::new(), "A(a: int32): A".to_owned())
    );
    assert_eq!(c.class("B"), "same(): never");
    // A constructor of no type makes nothing that has one.
    let c = checked("class A {\n  n = 0\n  constructor(n) {\n    this.n = n + 1\n  }\n}\nclass B extends A {}\nconst make = (n) => new A(n)\nconst b = new B(1)\n");
    assert_eq!(
        (c.found.clone(), c.of("A"), c.of("b")),
        (Vec::new(), "?".into(), "?".into())
    );
    // Outside a class, `this` and `super` are nothing; a rest is compared as a rest.
    assert_eq!(
        found("class {\n  m() {\n    return this.x\n  }\n}\nsuper.x\nsuper(1)\nconst t = this\n"),
        NONE
    );
    assert_eq!(
        found("class A {\n  m(...xs) {\n    return xs\n  }\n}\nclass B extends A {\n  m(...ys) {\n    return ys\n  }\n}\nnew A().m(1)\nnew B().m(2)\n"),
        NONE
    );
    // Two classes of a file, after what is known of classes already.
    let c = checked_knowing("const p = f()\nclass A {\n  y = 1\n}\nclass B {\n  z = \"s\"\n}\nconst v = p.x\nconst w = new A().y\nconst u = new B().z\n", true);
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.of("v"), c.of("w"), c.of("u")),
        ("number".into(), "int32".into(), "string".into())
    );
    assert_eq!(
        (c.class("Point"), c.class("A"), c.class("B")),
        ("x: number".into(), "y: int32".into(), "z: string".into())
    );
    let c = checked("class A {}\nclass B {}\nclass {}\nconst a = new A()\nconst b = new B()\n");
    assert_eq!((c.of("a"), c.of("b")), ("A".into(), "B".into()));
}

#[test]
fn a_class_is_one_class_in_every_module() {
    let out = program(
        &[
            ("a.js", "export class Box {\n  v\n  constructor(v) {\n    this.v = v\n  }\n}\nexport class Base {\n  n = 1\n}\n"),
            ("b.js", "import { Box, Base } from \"./a.js\"\nclass Sub extends Base {}\nconst b = new Box(1)\nconst v = b.v\nconst n = new Sub().n\nb.w = 1\n"),
        ],
        &["b.js"],
    );
    let of = |file: &str| out.iter().find(|(name, _)| name == file).expect("the file");
    let b = &of("b.js").1;
    assert_eq!(b.found, ["E129 @6:3 w"]);
    assert_eq!(
        (b.of("b"), b.of("v"), b.of("n")),
        ("Box<int32>".into(), "int32".into(), "int32".into())
    );
    assert_eq!(of("a.js").1.of("Box"), "Box<T>(a: T): Box<T>");
}
