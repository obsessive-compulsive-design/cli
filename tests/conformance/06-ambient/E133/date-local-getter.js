// expect: E133 removed-global-or-member @5:24
// The local getters depend on where the program runs: read the date as UTC.
const due = new Date("2026-01-31T12:00:00Z")

export const day = due.getDate()
