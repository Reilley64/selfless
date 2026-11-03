import { initials } from "./initials";

test("a one-word name gives its first letter", () => {
  expect(initials("Noor")).toBe("N");
});

test("a two-word name gives the first letter of each word", () => {
  expect(initials("Maya Ruiz")).toBe("MR");
});

test("a name of three words gives the first letters of the first two", () => {
  expect(initials("Ana Sofia Lima")).toBe("AS");
});

test("a lower-case name gives upper-case initials", () => {
  expect(initials("maya ruiz")).toBe("MR");
});

test("extra spaces around and between the words are ignored", () => {
  expect(initials("  Maya   Ruiz ")).toBe("MR");
});

test("a first letter outside the Basic Multilingual Plane stays whole", () => {
  expect(initials("𠮷田 Ana")).toBe("𠮷A");
});
