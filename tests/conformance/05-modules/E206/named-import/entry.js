// expect: E206 missing-export @2:13
import { a, b } from "./lib.js"

export const sum = a + b
