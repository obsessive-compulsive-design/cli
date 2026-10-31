// expect: S006 line-too-long @2:1
// expect: S003 import-order @4:1 "import { a } from \"./a.js\"\nimport { b } from \"./a/b.js\"\nimport { c } from \"../a.js\"" -> "import { c } from \"../a.js\"\nimport { a } from \"./a.js\"\nimport { b } from \"./a/b.js\""
// Depth first: `../a.js` before `./a.js`; then `./a.js` before `./a/b.js` by bytes.
import { a } from "./a.js"
import { b } from "./a/b.js"
import { c } from "../a.js"
a(b, c)
