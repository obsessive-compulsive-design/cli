// expect: E107 block-shadow @6:9
// An import is a binding of the module like any other.
import { size } from "./size.js"

{
  const size = 2
  size
}

export const kept = size
