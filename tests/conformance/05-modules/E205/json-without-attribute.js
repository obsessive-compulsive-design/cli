// expect: E205 unresolved-import @3:17
// A `.json` import needs `with { type: "json" }` (SPEC.md §5.1).
import cfg from "./c.json"
