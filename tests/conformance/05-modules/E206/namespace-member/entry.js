// expect: E206 missing-export @4:28
import * as ns from "./lib.js"

export const b = ns.a + ns.b
