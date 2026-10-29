// Every line start that is no hazard: no diagnostic. The first line is never one.
(first)
const o = {
}
const s = o
  .toString()
class A {}
(second)
{
  (third)
}
const t = () => {}
(fourth)
if (s) {
}
[a, b] = [b, a]
function f() {
  const v = 1
  return v
}
