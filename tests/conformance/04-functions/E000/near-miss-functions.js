// Every permitted function form: no diagnostic.
function f(a, b = 1, ...c) {
  return [a, b, c]
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
  return [a, c]
}

const l = () => ({})

const m = f((n) => n, 2)

await g(1)

export { h, i, j, k, l, m }
