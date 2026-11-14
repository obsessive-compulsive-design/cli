// expect: E205 unresolved-import @3:18
// An empty file holds no JSON value.
import data from "./data.json" with { type: "json" }

export const kept = data
