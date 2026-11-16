// A name may be declared once in each scope: every `case` body is a scope of its own, and
// a function declared in a block belongs to that block.
export const pick = (kind) => {
  switch (kind) {
    case 1: {
      const label = "one"
      return label
    }
    default: {
      const label = "other"
      return label
    }
  }
}

{
  function helper() {
    return 1
  }
  helper()
}

{
  function helper() {
    return 2
  }
  helper()
}
