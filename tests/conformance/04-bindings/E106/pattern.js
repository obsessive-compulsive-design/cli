// expect: E106 unused-let @5:1 "let" -> "const"
// Neither name of the pattern is assigned: one finding, on the `let`.
import { pair } from "./pair.js"

let [first, second] = pair

export { first, second }
