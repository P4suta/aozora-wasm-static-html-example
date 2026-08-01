import { describe, expect, test } from "bun:test";
import { splitAozoraFile } from "../src/envelope.ts";

const source = `作品名
著者名
--------------------
記号凡例
--------------------

本文です。

底本：「本」出版社
入力：入力者
`;

describe("splitAozoraFile", () => {
  test("splits the standard envelope", () => {
    expect(splitAozoraFile(source)).toEqual({
      preamble: "作品名\n著者名",
      legend: "記号凡例",
      body: "本文です。",
      bibliography: "底本：「本」出版社\n入力：入力者",
    });
  });

  test("normalizes CRLF and CR", () => {
    expect(splitAozoraFile(source.replaceAll("\n", "\r\n"))).toEqual(splitAozoraFile(source));
    expect(splitAozoraFile(source.replaceAll("\n", "\r"))).toEqual(splitAozoraFile(source));
  });

  test("rejects malformed section boundaries", () => {
    expect(() => splitAozoraFile(source.replace("--------------------\n", ""))).toThrow(
      "expected exactly two legend separators",
    );
    expect(() => splitAozoraFile(`${source}--------------------\n`)).toThrow(
      "expected exactly two legend separators",
    );
    expect(() => splitAozoraFile(source.replace("底本：", "書誌："))).toThrow(
      "missing terminal bibliography",
    );
  });

  test.each([
    ["preamble", source.replace("作品名\n著者名\n", "")],
    ["legend", source.replace("記号凡例", "")],
    ["body", source.replace("本文です。", "")],
  ])("rejects an empty %s", (name, input) => {
    expect(() => splitAozoraFile(input)).toThrow(`empty source ${name}`);
  });
});
