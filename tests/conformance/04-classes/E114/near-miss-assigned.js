// Every field has an initializer or is assigned straight down the constructor; one
// that starts as `undefined` and is given a string later is either: no diagnostic.
export class Session {
  static opened = 0
  id
  #token = undefined
  constructor(id) {
    this.id = id
  }
  open(token) {
    this.#token = token
    return this.#token
  }
}

export const session = new Session(1)

export const token = session.open("t")
