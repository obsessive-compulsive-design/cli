// ImportList: names, renames, and a trailing comma.
import { a } from "./a.js"
import { b as c, d, e as f, } from "./b.js"
a(c, d, f)
