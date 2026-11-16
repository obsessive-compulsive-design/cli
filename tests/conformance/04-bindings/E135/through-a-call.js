// expect: E135 use-before-declaration @3:22
// A declaration is hoisted, so `total` may be called here, but it reads `rate` too early.
export const early = total()

const rate = 2

function total() {
  return rate
}

export const later = total()
