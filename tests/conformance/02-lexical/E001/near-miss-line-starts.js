// Every line start that is no hazard: no diagnostic. The first line is never one.
(f() + 1).x

const o = {
  a: 1,
}

const s = f
  .toString()
  .trim()
  .toUpperCase()

class A {}

(f() + 2).x

{
  (f() + 3).x
}

const t = () => {}

(f() + 4).x

if (s) {
  f()
}

[o.a] = [2]

function f() {
  const v = 1
  return v
}

export { A, t }
