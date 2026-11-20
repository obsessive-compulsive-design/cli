// Block: declarations and statements inside braces, nested.
import { f } from "./names.js"

{
  const a = 1
  {
    let b = a
    b += 1
    f(b)
  }

  function g() {}
  class C {}
  g(new C())
}
