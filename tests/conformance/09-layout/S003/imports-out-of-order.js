// expect: S003 import-order @3:1 "import { b } from \"./b.js\"\nimport { a } from \"./a.js\"" -> "import { a } from \"./a.js\"\nimport { b } from \"./b.js\""
// Two imports from one directory sort by path: `./a.js` before `./b.js`.
import { b } from "./b.js"
import { a } from "./a.js"
a(b)
