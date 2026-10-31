// FunctionDecl: plain, async, generator and async generator, with every parameter form.
function f(a, b = 1, ...rest) {
  return [a, b, rest]
}

async function g() {
  await f(1)
}

function* h() {
  yield 1
}

async function* k() {
  yield await g()
}
