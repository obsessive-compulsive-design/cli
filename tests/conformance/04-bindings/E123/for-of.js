// expect: E123 modified-loop-variable @7:5
// The body assigns the variable the loop fills.
import { xs } from "./xs.js"

const run = () => {
  for (let x of xs) {
    x = 1
    console.log(x)
  }
}

export { run }
