// ForOfStmt: `const` bindings, patterns, and `for await`.
import { f } from "./names.js"

const xs = [1, 2]

const m = new Map([[1, 2]])

for (const x of xs) {
  f(x)
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
