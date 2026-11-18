// expect: E138 type-mismatch @7:5
// A `const` loop variable is a constant like any other: E138, not E123.
import { xs } from "./xs.js"

const run = () => {
  for (const x of xs) {
    x = 1
    console.log(x)
  }
}

export { run }
