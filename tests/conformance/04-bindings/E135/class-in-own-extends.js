// expect: E135 use-before-declaration @3:27
// The heritage runs before the class has its name.
export class Node extends Node {
  static root = new Node()
}
