// Two calls on one line, three calls one per line, and accesses alone: no diagnostic.
import { a, f } from "./names.js"

const x = a.b().c()

const y = a
  .b()
  .c()
  .d()

const z = a.b.c.d

const w = f().a().b()

export { x, y, z, w }
