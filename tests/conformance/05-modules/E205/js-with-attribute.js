// expect: E205 unresolved-import @3:19
// The attribute belongs on a `.json` import only.
import { a } from "./a.js" with { type: "json" }
