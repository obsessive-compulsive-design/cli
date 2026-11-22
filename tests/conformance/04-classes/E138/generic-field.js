// expect: E138 type-mismatch @14:13
// `Box` is generic in what it is made with: this one holds a number.
export class Box {
  value
  constructor(value) {
    this.value = value
  }
}

export const box = new Box(1)

export const text = new Box("a")

box.value = "b"
