// TryStmt: catch with and without a binding, finally with and without catch.
import { f } from "./names.js"

try {
  f(1)
} catch {
  f(2)
}

try {
  f(3)
} catch (e) {
  f(e)
} finally {
  f(4)
}

try {
  f(5)
} finally {
  f(6)
}
