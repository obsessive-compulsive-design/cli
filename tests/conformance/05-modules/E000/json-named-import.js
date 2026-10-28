// expect: E000 syntax @3:19
// A JSON module has no named exports.
import { k } from "./c.json" with { type: "json" }
