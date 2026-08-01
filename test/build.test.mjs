import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { parse } from "parse5";
import { buildSite } from "../src/build.mjs";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

async function filesUnder(root, prefix = "") {
  const entries = await readdir(join(root, prefix), { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const relative = join(prefix, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(root, relative)));
    else files.push(relative);
  }
  return files.sort();
}

async function directoryHashes(root) {
  const hashes = {};
  for (const file of await filesUnder(root)) {
    hashes[file] = createHash("sha256").update(await readFile(join(root, file))).digest("hex");
  }
  return hashes;
}

test("builds ten deterministic, diagnostic-free HTML pages", async () => {
  const first = await mkdtemp(join(tmpdir(), "aozora-html-first-"));
  const second = await mkdtemp(join(tmpdir(), "aozora-html-second-"));
  const firstReport = await buildSite({ root: ROOT, outDir: first });
  const secondReport = await buildSite({ root: ROOT, outDir: second });

  assert.deepEqual(firstReport, secondReport);
  assert.equal(firstReport.generator.version, "0.5.0");
  assert.equal(firstReport.works.length, 10);
  assert.ok(firstReport.works.every(({ diagnosticCount }) => diagnosticCount === 0));
  assert.deepEqual(await directoryHashes(first), await directoryHashes(second));

  const htmlFiles = (await filesUnder(first)).filter((file) => file.endsWith(".html"));
  assert.equal(htmlFiles.length, 11);
  for (const file of htmlFiles) {
    const html = await readFile(join(first, file), "utf8");
    const parseErrors = [];
    parse(html, { onParseError: (error) => parseErrors.push(error) });
    assert.deepEqual(parseErrors, [], file);
    assert.doesNotMatch(html, /<script\b/i, file);
  }

  const aibiki = await readFile(join(first, "000005.utf8.html"), "utf8");
  assert.match(aibiki, /<ruby>/);
  assert.match(aibiki, /data-codepoint="U\+6491">撑/);
  assert.match(aibiki, /底本：/);
  assert.doesNotMatch(aibiki, /テキスト中に現れる記号について/);
  assert.doesNotMatch(aibiki, /Char\('/);
});
