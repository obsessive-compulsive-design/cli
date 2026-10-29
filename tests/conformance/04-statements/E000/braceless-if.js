// expect: E000 syntax @3:8 "y()" -> "{ y() }"
// Every statement body is a block in braces (SPEC.md §4.3).
if (x) y()
