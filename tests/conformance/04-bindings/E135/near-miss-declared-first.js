// Each of these runs after what it names is declared.
function total() {
  return rate
}

const rate = 2

export const later = total()

export const lazy = () => after

export { after }

const after = hoisted()

function hoisted() {
  return 1
}

export const area = (width, height = width) => height * width

export class Tree {
  static root = new Tree()

  grow() {
    return [Tree, last]
  }
}

const last = 1

export const [head, next = head] = [last]
