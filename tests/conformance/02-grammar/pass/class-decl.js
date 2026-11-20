// ClassDecl: a class, one extending a name, and one extending a namespaced name.
import * as ns from "./d.js"
import { f } from "./names.js"

class A {}

class B extends A {}

class C extends ns.D {}

f(new B(), new C())
