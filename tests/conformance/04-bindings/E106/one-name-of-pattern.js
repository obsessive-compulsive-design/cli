// expect: E106 unused-let @5:13
// `first` is reassigned and `second` is not: the finding is on `second`.
import { pair } from "./pair.js"

let [first, second] = pair

first += 1

export { first, second }
