// An override that goes where the overridden method does, a setter that takes what
// its getter gives, a class with `toString` in a template, a class as a value: no
// diagnostic.
export class Shape {
  #scale = 1
  get scale() {
    return this.#scale
  }
  set scale(scale) {
    this.#scale = scale
  }
  area() {
    return 0
  }
  toString() {
    return "shape"
  }
}

export class Square extends Shape {
  side
  constructor(side) {
    super()
    this.side = side
  }
  area() {
    return this.side * this.side
  }
}

export const square = new Square(2)

square.scale = 3

export const described = `${square}: ${square.area()}`

export const kinds = [Shape, Square]
