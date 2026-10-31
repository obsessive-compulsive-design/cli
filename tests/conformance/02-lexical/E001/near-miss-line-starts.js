// Every line start that is no hazard: no diagnostic. The first line is never one.
(first + 1).x

const o = {
  a: 1,
}

const s = o
  .toString()
  .trim()
  .toUpperCase()

class A {}

(second + 1).x

{
  (third + 1).x
}

const t = () => {}

(fourth + 1).x

if (s) {
  f()
}

[a, b] = [b, a]

function f() {
  const v = 1
  return v
}
