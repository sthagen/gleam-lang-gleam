class Record {
  inspect() {
    let field = (label) => {
      let value = inspect(this[label]);
      return isNaN(parseInt(label)) ? `${label}: ${value}` : value;
    };
    let props = Object.getOwnPropertyNames(this).map(field).join(", ");
    return props ? `${this.constructor.name}(${props})` : this.constructor.name;
  }
}

class List {
  inspect() {
    return `[${this.toArray().map(inspect).join(", ")}]`;
  }

  static fromArray(array) {
    return array.reduceRight(
      (list, element) => new NonEmpty(element, list),
      new Empty()
    );
  }

  toArray() {
    let current = this;
    let array = [];
    while (current instanceof NonEmpty) {
      array.push(current.head);
      current = current.tail;
    }
    return array;
  }

  atLeastLength(desired) {
    let current = this;
    while (current instanceof NonEmpty) {
      if (desired <= 0) return true;
      desired--;
      current = current.tail;
    }
    return desired <= 0;
  }
}

class Empty extends List {}

class NonEmpty extends List {
  constructor(head, tail) {
    super();
    this.head = head;
    this.tail = tail;
  }
}

class BitString {
  constructor(buffer) {
    this.buffer = buffer;
  }

  inspect() {
    return `<<${Array.from(this.buffer).join(", ")}>>`;
  }

  size() {
    return this.buffer.length;
  }
}

class UtfCodepoint {
  constructor(value) {
    this.value = value;
  }

  toBuffer() {
    return new Uint8Array(String.fromCodePoint(this.value));
  }

  inspect() {
    return `//utf8codepoint(${String.fromCodePoint(this.value)})`;
  }
}

class Result extends Record {
  isOk() {
    return this instanceof Ok;
  }
}

class Ok extends Result {
  constructor(value) {
    super();
    this[0] = value;
  }
}

class Error extends Result {
  constructor(detail) {
    super();
    this[0] = detail;
  }
}

class Thing extends Record {
  constructor(first, detail, boop) {
    super();
    this[0] = first;
    this.detail = detail;
    this.boop = boop;
  }
}

function inspect(v) {
  let t = typeof v;
  if (v === true) return "True";
  if (v === false) return "False";
  if (v === null) return "//js(null)";
  if (v === undefined) return "Nil";
  if (t === "string") return JSON.stringify(v);
  if (t === "bigint" || t === "number") return v.toString();
  if (Array.isArray(v)) return `#(${v.map(inspect).join(", ")})`;
  if (v instanceof globalThis.Error)
    return `//js(new ${v.constructor.name}(${inspect(v.message)}))`;
  try {
    if (typeof v.inspect === "function") return v.inspect();
  } catch (error) {}
  let properties = Object.entries(v).map(([k, v]) => `${k}: ${inspect(v)}`);
  return `//js({${properties.join(", ")}})`;
}

function equal(x, y) {
  let values = [x, y];
  while (values.length !== 0) {
    let a = values.pop();
    let b = values.pop();
    if (a === b || uintArrayEqual(a, b)) continue;
    if (objectType(a) !== objectType(b)) return false;
    for (let k of Object.keys(a)) values.push(a[k], b[k]);
  }
  return true;
}

function uintArrayEqual(a, b) {
  return (
    a instanceof Uint8Array &&
    b instanceof Uint8Array &&
    a.byteLength === b.byteLength &&
    a.every((x, i) => x === b[i])
  );
}

function objectType(object) {
  if (object !== null && typeof object === "object") {
    return object.constructor.name;
  } else {
    return typeof object;
  }
}

console.log("");
console.log("");

let x = new Record();
console.log(x.inspect());

console.log(inspect(true));
console.log(inspect(false));
console.log(inspect(undefined));

let ok = new Ok(1);
console.log(ok.inspect());

let error = new Error("err");
console.log(error.inspect());

let thing = new Thing([1, 2, "hello"], "ok", { a: 1 });
console.log(thing.inspect());

console.log(List.fromArray([]).inspect());
console.log(
  List.fromArray([
    1,
    2,
    new Ok([1]),
    null,
    new globalThis.Error("stuff"),
    { gl: new Ok(123) },
  ]).inspect()
);

console.log(inspect(new BitString(new Uint8Array([1, 2, 3]))));

console.log(inspect(new UtfCodepoint(128013)));
