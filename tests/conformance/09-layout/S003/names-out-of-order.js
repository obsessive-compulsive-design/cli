// expect: S006 line-too-long @2:1
// expect: S003 import-order @4:1 "import { b as a, a as b } from \"./x.js\"" -> "import { a as b, b as a } from \"./x.js\""
// Names sort by the imported name, not the local one.
import { b as a, a as b } from "./x.js"
a(b)
