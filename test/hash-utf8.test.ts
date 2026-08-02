import { describe, expect, test } from "bun:test";
import { sha256, verifySha256 } from "../src/hash.ts";
import { decodeUtf8 } from "../src/utf8.ts";

describe("content integrity utilities", () => {
  test("hashes strings and bytes identically", () => {
    const expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    expect(sha256("abc")).toBe(expected);
    expect(sha256(new TextEncoder().encode("abc"))).toBe(expected);
    expect(verifySha256(new TextEncoder().encode("abc"), expected, "fixture")).toBe(expected);
  });

  test("reports both integrity values on mismatch", () => {
    expect(() => verifySha256(new Uint8Array([1]), "0".repeat(64), "fixture")).toThrow(
      /fixture SHA-256 mismatch: expected 0{64}, received [0-9a-f]{64}/,
    );
  });

  test("decodes valid UTF-8 without replacement", () => {
    expect(decodeUtf8(new TextEncoder().encode("青空"), "fixture")).toBe("青空");
  });

  test("rejects malformed UTF-8 and an encoded replacement character", () => {
    expect(() => decodeUtf8(new Uint8Array([0xc3, 0x28]), "fixture")).toThrow(
      "fixture is not valid UTF-8",
    );
    expect(() => decodeUtf8(new TextEncoder().encode("a\uFFFDb"), "fixture")).toThrow(
      "fixture contains a Unicode replacement character",
    );
  });
});
