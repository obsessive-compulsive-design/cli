// A string literal names the export: no diagnostic.
import * as ns from "./lib.js"

export const b = ns["a"]
