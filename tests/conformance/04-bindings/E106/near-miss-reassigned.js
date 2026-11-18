// Every way to reassign, a nested function and dead code included: no diagnostic.
let a = 1

let b = 1

let c = 1

let d = 1

let e = 1

let f = 1

a = 2

b += 1

c ??= 2

const bump = () => {
  d -= 1
}

if (a > b) {
  [e, f] = [f, e]
}

for (let i = 0; i < a; i += 1) {
  bump()
}

export { a, b, c, d, e, f }
