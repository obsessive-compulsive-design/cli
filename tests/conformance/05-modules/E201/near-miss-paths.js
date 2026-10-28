// Relative paths that look odd but are left to resolution (US-060): no diagnostic.
import { a } from "./a.js/"
import { b } from "./"
import { c } from "././c.js"
import { d } from "../../d.js"
import cfg from "../c.json" with { type: "json" }
