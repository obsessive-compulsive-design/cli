// expect: E138 type-mismatch @7:21
// A shape is not iterable: it has no `[Symbol.iterator]`.
const limits = { low: 1, high: 2 }

const seen = [0]

for (const limit of limits) {
  seen.push(limit)
}

export { seen }
