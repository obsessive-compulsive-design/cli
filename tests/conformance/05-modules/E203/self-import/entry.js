// expect: E203 import-cycle @3:22
// The smallest cycle: a module that imports itself.
import { kept } from "./entry.js"

export const other = kept
