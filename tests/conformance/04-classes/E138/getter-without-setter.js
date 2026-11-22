// expect: E138 type-mismatch @12:8
// `area` has a getter and no setter: it is read, not written.
export class Square {
  side = 2
  get area() {
    return this.side * this.side
  }
}

export const square = new Square()

square.area = 9
