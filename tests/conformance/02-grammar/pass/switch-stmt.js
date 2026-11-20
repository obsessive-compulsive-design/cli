// SwitchStmt: clauses, a default, and an empty switch.
import { f } from "./names.js"

const x = 1

switch (x) {
  case 1: {
    f(1)
    break
  }
  case 2:
  case 3: {
    f(2)
    break
  }
  default: {
    f(0)
  }
}

switch (x) {}
