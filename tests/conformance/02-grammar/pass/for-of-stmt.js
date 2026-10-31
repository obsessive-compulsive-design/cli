// ForOfStmt: `const` and `let` bindings, patterns, and `for await`.
const xs = [1, 2]

const m = new Map()

for (const x of xs) {
  f(x)
}

for (let y of xs) {
  y += 1
  f(y)
}

for (const [k, v] of m) {
  f(k, v)
}

for (const { a } of xs) {
  f(a)
}

for await (const z of xs) {
  f(z)
}
