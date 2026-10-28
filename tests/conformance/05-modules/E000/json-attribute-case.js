// expect: E000 syntax @3:41
// The attribute is exactly `type: "json"`.
import cfg from "./c.json" with { type: "JSON" }
