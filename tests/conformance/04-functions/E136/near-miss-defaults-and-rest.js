// A default may be left out and a rest takes any number more: no diagnostic.
const add = (a, b = 1) => a + b

const count = (first, ...others) => others.length + first

export const sums = [add(1), add(1, 2), count(1), count(1, 2, 3)]
