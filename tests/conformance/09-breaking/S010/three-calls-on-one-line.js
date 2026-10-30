// expect: S010 method-chain @2:11 "a.b().c().d()" -> "a\n  .b()\n  .c()\n  .d()"
const x = a.b().c().d()
