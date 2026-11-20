// ForStmt: `let` counter, condition, update.
import { f } from "./names.js"

const n = 3

for (let i = 0; i < n; i += 1) {
  f(i)
}

for (let j = n; j > 0; j -= 1) {
  f(j)
}
