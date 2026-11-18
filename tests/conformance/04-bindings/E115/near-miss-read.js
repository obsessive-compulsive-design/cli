// Each binding is read, exported, or named as one nothing reads.
import { size } from "./size.js"

class Shape {}

function area() {
  return size
}

export const isShape = (value, _options) => value instanceof Shape

export const measure = [area]

export const attempt = (run) => {
  try {
    return run()
  } catch {
    return 0
  }
}

const _spare = 1

const { _dropped, ...others } = measure

export { others }
