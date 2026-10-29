// expect: E000 syntax @3:8 "f()" -> "{ f() }"
// Braceless statement bodies are removed: a block in braces, always.
if (x) f()
