// ArrowFn: parameters always parenthesised, expression or block body, async or not.
const a = (x) => x
const b = () => {}
const c = (x, y = 1, ...z) => [x, y, z]
const d = async (p) => await p
const e = async () => {
  return 1
}
f(a, b, c, d, e)
