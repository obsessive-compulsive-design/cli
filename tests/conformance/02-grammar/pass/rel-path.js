// RelPath: `./` and `../` paths ending in `.js`, through subdirectories.
import { a } from "./a.js"
import { b } from "../b.js"
import { c } from "./sub/dir/c.js"
a(b, c)
