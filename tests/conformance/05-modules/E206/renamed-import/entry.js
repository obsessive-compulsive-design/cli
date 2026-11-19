// expect: E206 missing-export @3:10
// The name asked for is `b`, whatever it is called here.
import { b as c } from "./lib.js"

export const d = c
