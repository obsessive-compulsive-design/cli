# Platform globals (SPEC.md §6.2)

Each file transcribes by hand the Web IDL of the portable platform globals, from the
standard its header cites. The checker types the globals from these files; nothing
else says what they are.

A table is one global, or an interface a global gives:

    [Headers]              an interface: a class of the ambient library
    [namespace console]    an object with members, which is not constructed
    [function fetch]       a function

Its keys:

    idl      = "…"         the standard and the IDL definition transcribed (required)
    extends  = "Name"      the interface it inherits from
    sample   = "…"         an expression that gives an instance, for the generated
                           fixtures: an interface without one is not constructed by
                           a program
    new      = "(…)"       the parameters of its constructor
    call     = "(…) => T"  the signature of a function
    name     = "(…) => T"  a method
    name     = "T"         an attribute
    static name = "…"      a static member, which `ambient/globals.toml` lists too

The types: `string`, `number`, `int32`, `bigint`, `boolean`, `undefined`, `null`,
`unknown` (IDL `any`), `T[]`, `[A, B]`, `A | B`, `Promise<T>`, `Iterator<T>`,
`AsyncIterator<T>`, `(name: T, other?: U, ...rest: V) => R`, and the name of an
interface of these files or of a typed array or buffer.

Where the IDL type is nullable (`T?`), the member is typed `T | null` and its line
ends with `# nullable`: a test holds the two together.
