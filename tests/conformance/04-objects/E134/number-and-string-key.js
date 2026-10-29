// expect: E134 duplicate-key @3:21
// `1` and `"1"` spell the same key, as ES compares them.
const o = { 1: "a", "1": "b" }
