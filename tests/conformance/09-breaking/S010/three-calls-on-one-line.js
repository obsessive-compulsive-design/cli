// expect: S010 method-chain @4:11 "a.b().c().d()" -> "a\n  .b()\n  .c()\n  .d()"
import { a } from "./names.js"

const x = a.b().c().d()

export { x }
