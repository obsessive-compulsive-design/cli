// A `let` and a parameter may be assigned; so may a member of a constant: no diagnostic.
const box = { size: 1 }

let limit = 1

limit = 2

const grow = (by) => {
  by += limit
  box.size = by
}

export { grow }
