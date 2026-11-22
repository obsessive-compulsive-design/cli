// expect: E138 type-mismatch @7:30
// `base` is a function: a class extends a class.
function base() {
  return 1
}

export class Derived extends base {}
