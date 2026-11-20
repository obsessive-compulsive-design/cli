// A block with two statements, a bare `return`, or a comment: no diagnostic.
import { a, g } from "./names.js"

const f = () => {
  g()
  return a
}

const h = () => {
  return
}

const i = () => {
  // why
  return a
}

export { f, h, i }
