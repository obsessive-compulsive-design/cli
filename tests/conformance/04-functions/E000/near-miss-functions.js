// Every permitted function form: no diagnostic.
function f(a, b = 1, ...c) {
  return a
}
async function g(x) {
  return await x
}
function* h() {
  yield 1
}
async function* i() {
  yield await 1
}
const j = (a) => a
const k = async (a, { b }, ...c) => {
  g(b)
  return a
}
const l = () => ({})
const m = f((n) => n, 2)
await g(1)
