// expect: E205 unresolved-import @3:24
// A JSON module is imported statically, with `with { type: "json" }`.
const c = await import("./c.json")
