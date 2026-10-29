// ImportDecl: a named list, a namespace, and the one default import, of JSON.
import { a, b as c } from "./a.js"
import * as ns from "../lib/b.js"
import cfg from "./c.json" with { type: "json" }
a(c, ns, cfg)
