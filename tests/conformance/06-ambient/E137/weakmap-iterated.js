// expect: E137 missing-property @9:21
// A `WeakMap` cannot be iterated: its keys may be gone at any time.
const key = { id: 1 }

const seen = new WeakMap([[key, 1]])

const found = [0]

for (const entry of seen) {
  found.push(entry.length)
}

export { found }
