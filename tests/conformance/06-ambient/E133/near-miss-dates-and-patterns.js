// A regex literal with its groups, a `Date` from ISO 8601 text read as UTC, an error
// with a cause, a class that extends `Error`, and `Intl` in place of `localeCompare`:
// no diagnostic.
class ParseError extends Error {
  line = 0
}

const stamp = /(?<year>\d{4})-(?<month>\d\d)/dv

const found = stamp.exec("2026-01")

const due = new Date("2026-01-31T12:00:00Z")

const collator = new Intl.Collator("en")

export const report = [
  found?.[0] ?? "",
  found?.index ?? 0,
  due.getUTCFullYear(),
  due.toISOString(),
  Date.now(),
  ["b", "a"].toSorted(collator.compare).join(","),
  new ParseError("no", { cause: new Error("why") }).message,
]
