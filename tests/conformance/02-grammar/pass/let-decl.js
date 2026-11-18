// LetDecl: always with an initializer.
const o = { a: 1 }

const xs = [1]

let a = 1

let { b } = o

let [c] = xs

a += b + c

b += 1

c += 1

export { a }
