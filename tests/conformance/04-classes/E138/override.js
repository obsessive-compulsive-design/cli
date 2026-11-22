// expect: E138 type-mismatch @11:3
// `Loud.name` returns a number where `Animal.name` returns a string: a `Loud` does
// not go where an `Animal` does.
export class Animal {
  name() {
    return "animal"
  }
}

export class Loud extends Animal {
  name() {
    return 1
  }
}
