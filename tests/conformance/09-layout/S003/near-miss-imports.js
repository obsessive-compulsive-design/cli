// Imports in order: by depth, then by path, the JSON `with` clause and the kind of
// import not counting; two imports from one path stay two; names in order: no diagnostic.
import { d } from "../../d.js"
import * as c from "../c.js"
import { a, b as x } from "./a.js"
import { a as y } from "./a.js"
import cfg from "./a/cfg.json" with { type: "json" }
import { e } from "./e.js"
d(c, a, x, y, cfg, e)
