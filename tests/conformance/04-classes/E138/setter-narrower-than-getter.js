// expect: E138 type-mismatch @8:7
// The getter gives a string, and the setter is only ever given a number.
export class Label {
  #text = "a"
  get text() {
    return this.#text
  }
  set text(size) {
    this.#text = `${size}`
  }
}

export const label = new Label()

label.text = 12
