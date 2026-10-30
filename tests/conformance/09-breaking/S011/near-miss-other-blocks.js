// A block with two statements, a bare `return`, or a comment: no diagnostic.
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
