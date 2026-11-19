// expect: E206 missing-export @4:21
import * as ns from "./lib.js"

export const b = ns["b"]
