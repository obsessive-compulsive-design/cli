// expect: E106 unused-let @6:8 "let" -> "const"
// The loop gives `x` its value; nothing else assigns it.
import { xs } from "./xs.js"

const seen = () => {
  for (let x of xs) {
    console.log(x)
  }
}

export { seen }
