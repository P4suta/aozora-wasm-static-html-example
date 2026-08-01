import assert from "node:assert/strict";
import test from "node:test";
import { splitAozoraFile } from "../src/envelope.mjs";

const LF_SOURCE = `作品名
著者名
--------------------
記号凡例
--------------------

本文です。

底本：「本」出版社
入力：入力者
`;

test("splits the standard Aozora file envelope", () => {
  assert.deepEqual(splitAozoraFile(LF_SOURCE), {
    preamble: "作品名\n著者名",
    legend: "記号凡例",
    body: "本文です。",
    bibliography: "底本：「本」出版社\n入力：入力者",
  });
});

test("accepts CRLF without changing the sections", () => {
  assert.deepEqual(
    splitAozoraFile(LF_SOURCE.replaceAll("\n", "\r\n")),
    splitAozoraFile(LF_SOURCE),
  );
});

test("rejects missing and extra separators", () => {
  assert.throws(() => splitAozoraFile(LF_SOURCE.replace("--------------------\n", "")));
  assert.throws(() => splitAozoraFile(`${LF_SOURCE}--------------------\n`));
});

test("rejects a missing bibliography and an empty body", () => {
  assert.throws(() => splitAozoraFile(LF_SOURCE.replace("底本：", "書誌：")));
  assert.throws(() => splitAozoraFile(LF_SOURCE.replace("本文です。", "")));
});
