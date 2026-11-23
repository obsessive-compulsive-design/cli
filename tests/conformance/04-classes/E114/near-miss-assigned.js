// Every field has an initializer or is assigned on every path through the
// constructor, a path that throws aside; one that starts as `undefined` and is given a
// string later is either: no diagnostic.
export class Session {
  static opened = 0
  id
  kind
  #token = undefined
  constructor(id) {
    if (id < 0) {
      throw new Error("no such session")
    }
    this.id = id
    if (id === 0) {
      this.kind = "root"
    } else {
      this.kind = "user"
    }
  }
  open(token) {
    this.#token = token
    return this.#token
  }
}

export const session = new Session(1)

export const token = session.open("t")
