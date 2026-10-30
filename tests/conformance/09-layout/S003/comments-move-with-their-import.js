// expect: S003 import-order @3:1 "import { c } from \"./c.js\"\n// about b\nimport { b } from \"./b.js\" // b's line\nimport { a } from \"./a.js\"" -> "import { a } from \"./a.js\"\n// about b\nimport { b } from \"./b.js\"  // b's line\nimport { c } from \"./c.js\""
// A comment above an import and the one at the end of its line move with it.
import { c } from "./c.js"
// about b
import { b } from "./b.js" // b's line
import { a } from "./a.js"
a(b, c)
