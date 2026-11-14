// expect: E205 unresolved-import @3:18
// `data.json` has a trailing comma: it is not JSON, so it is no module.
import data from "./data.json" with { type: "json" }

export const kept = data
