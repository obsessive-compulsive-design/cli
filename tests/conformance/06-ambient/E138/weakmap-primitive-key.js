// expect: E138 type-mismatch @5:10
// A `WeakMap` holds its keys weakly: a key is an object, never a string.
const seen = new WeakMap()

seen.set("first", 1)

export { seen }
