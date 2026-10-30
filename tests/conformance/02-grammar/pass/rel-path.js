// RelPath: `./` and `../` paths ending in `.js`, through subdirectories.
import { b } from "../b.js"
import { a } from "./a.js"
import { c } from "./sub/dir/c.js"
a(b, c)
