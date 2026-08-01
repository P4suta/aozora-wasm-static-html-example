import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { copyFile, cp, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { type DefaultTreeAdapterTypes, type ParserError, parse } from "parse5";
import { buildSite } from "../src/build.ts";
import { sha256 } from "../src/hash.ts";

const root = fileURLToPath(new URL("..", import.meta.url));

async function filesUnder(directory: string, prefix = ""): Promise<ReadonlyArray<string>> {
  const entries = await readdir(join(directory, prefix), { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    const relative = join(prefix, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(directory, relative)));
    else files.push(relative);
  }
  return files.sort();
}

async function directoryHashes(directory: string): Promise<Record<string, string>> {
  const hashes: Record<string, string> = {};
  for (const file of await filesUnder(directory)) {
    hashes[file] = createHash("sha256")
      .update(await readFile(join(directory, file)))
      .digest("hex");
  }
  return hashes;
}

function childElements(
  node: DefaultTreeAdapterTypes.ParentNode,
): ReadonlyArray<DefaultTreeAdapterTypes.Element> {
  const result: DefaultTreeAdapterTypes.Element[] = [];
  for (const child of node.childNodes) {
    if ("tagName" in child) {
      result.push(child);
      result.push(...childElements(child));
    }
  }
  return result;
}

function attribute(element: DefaultTreeAdapterTypes.Element, name: string): string | undefined {
  return element.attrs.find((item) => item.name === name)?.value;
}

function textContent(node: DefaultTreeAdapterTypes.Node): string {
  if (node.nodeName === "#text" && "value" in node) return node.value;
  if ("childNodes" in node) return node.childNodes.map(textContent).join("");
  return "";
}

function rubyBase(ruby: DefaultTreeAdapterTypes.Element): string {
  return ruby.childNodes
    .filter((child) => !("tagName" in child) || (child.tagName !== "rt" && child.tagName !== "rp"))
    .map(textContent)
    .join("");
}

async function fixtureRoot(): Promise<string> {
  const fixture = await mkdtemp(join(tmpdir(), "aozora-static-fixture-"));
  await Promise.all([
    mkdir(join(fixture, "src"), { recursive: true }),
    mkdir(join(fixture, "vendor"), { recursive: true }),
    cp(join(root, "sources"), join(fixture, "sources"), { recursive: true }),
    copyFile(join(root, "works.json"), join(fixture, "works.json")),
    copyFile(join(root, "LICENSE-APACHE"), join(fixture, "LICENSE-APACHE")),
    copyFile(join(root, "LICENSE-MIT"), join(fixture, "LICENSE-MIT")),
  ]);
  await Promise.all([
    copyFile(join(root, "src", "site.css"), join(fixture, "src", "site.css")),
    copyFile(
      join(root, "vendor", "aozora-notation.css"),
      join(fixture, "vendor", "aozora-notation.css"),
    ),
    copyFile(
      join(root, "vendor", "aozora-notation.json"),
      join(fixture, "vendor", "aozora-notation.json"),
    ),
  ]);
  return fixture;
}

describe("static site build", () => {
  test("builds ten deterministic, diagnostic-free semantic HTML pages", async () => {
    const temporary = await mkdtemp(join(tmpdir(), "aozora-static-build-"));
    const first = join(temporary, "first");
    const second = join(temporary, "second");
    try {
      const firstReport = await buildSite({ root, outDir: first });
      const secondReport = await buildSite({ root, outDir: second });

      expect(firstReport).toEqual(secondReport);
      expect(firstReport.generator).toEqual({ package: "aozora-wasm", version: "0.5.0" });
      expect(firstReport.notationCss.sha256).toBe(
        "57cca99de485fa901800af204a1800ff880e00f5297f2aac2966b42678a104bc",
      );
      expect(firstReport.works).toHaveLength(10);
      expect(firstReport.works.every(({ diagnosticCount }) => diagnosticCount === 0)).toBeTrue();
      expect(await directoryHashes(first)).toEqual(await directoryHashes(second));

      const htmlFiles = (await filesUnder(first)).filter((file) => file.endsWith(".html"));
      expect(htmlFiles).toHaveLength(11);
      for (const file of htmlFiles) {
        const source = await readFile(join(first, file), "utf8");
        const parseErrors: ParserError[] = [];
        parse(source, { onParseError: (error) => parseErrors.push(error) });
        expect(parseErrors, file).toEqual([]);
        expect(source, file).not.toMatch(/<script\b/i);
        expect(source.indexOf("aozora-notation.css"), file).toBeLessThan(
          source.indexOf("site.css"),
        );
      }

      const aibiki = await readFile(join(first, "000005.utf8.html"), "utf8");
      expect(aibiki).toContain("<ruby>");
      expect(aibiki).toMatch(/data-codepoint="U\+6491">撑/);
      expect(aibiki).toContain("底本：");
      expect(aibiki).not.toContain("テキスト中に現れる記号について");
      expect(aibiki).not.toContain("Char('");
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  }, 30_000);

  test("makes the aozora-wasm 0.5.0 mixed-gaiji ruby boundary explicit", async () => {
    const temporary = await mkdtemp(join(tmpdir(), "aozora-ruby-boundary-"));
    try {
      const report = await buildSite({ root, outDir: temporary });
      const source = await readFile(join(temporary, "000092.utf8.html"), "utf8");
      const document = parse(source);
      const elements = childElements(document);
      const ruby = elements.find(
        (element) =>
          element.tagName === "ruby" &&
          element.childNodes.some(
            (child) =>
              "tagName" in child && child.tagName === "rt" && textContent(child) === "かんだた",
          ),
      );
      expect(ruby).toBeDefined();
      if (ruby === undefined) throw new Error("かんだた ruby is missing");

      const base = rubyBase(ruby);
      if (report.generator.version === "0.5.0") {
        expect(base).toBe("陀多");
        const gaiji = elements.find(
          (element) =>
            element.tagName === "span" && attribute(element, "data-codepoint") === "U+728D",
        );
        expect(gaiji).toBeDefined();
        expect(gaiji === undefined ? "" : textContent(gaiji)).toBe("犍");
      } else {
        expect(base).toBe("犍陀多");
      }
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  }, 30_000);

  test("verifies vendored CSS provenance before replacing existing output", async () => {
    const fixture = await fixtureRoot();
    const outDir = join(fixture, "dist");
    try {
      await mkdir(outDir);
      await writeFile(join(outDir, "sentinel"), "preserve");
      await writeFile(join(fixture, "vendor", "aozora-notation.css"), "tampered");
      await expect(buildSite({ root: fixture, outDir })).rejects.toThrow(
        "vendor/aozora-notation.css SHA-256 mismatch",
      );
      expect(await readFile(join(outDir, "sentinel"), "utf8")).toBe("preserve");
    } finally {
      await rm(fixture, { recursive: true, force: true });
    }
  });

  test("uses fatal UTF-8 decoding and cleans staging without replacing existing output", async () => {
    const fixture = await fixtureRoot();
    const outDir = join(fixture, "dist");
    try {
      await mkdir(outDir);
      await writeFile(join(outDir, "sentinel"), "preserve");
      const manifest = JSON.parse(await readFile(join(fixture, "works.json"), "utf8")) as {
        works: Array<{ id: string; sourceSha256: string }>;
      };
      const first = manifest.works[0];
      if (first === undefined) throw new Error("fixture is incomplete");
      const invalid = new Uint8Array([0xc3, 0x28]);
      first.sourceSha256 = sha256(invalid);
      await Promise.all([
        writeFile(join(fixture, "works.json"), `${JSON.stringify(manifest, null, 2)}\n`),
        writeFile(join(fixture, "sources", `${first.id}.txt`), invalid),
      ]);

      await expect(buildSite({ root: fixture, outDir })).rejects.toThrow(
        `${first.id} source is not valid UTF-8`,
      );
      expect(await readFile(join(outDir, "sentinel"), "utf8")).toBe("preserve");
      const leftovers = (await readdir(fixture)).filter((name) => name.startsWith(".dist.stage-"));
      expect(leftovers).toEqual([]);
    } finally {
      await rm(fixture, { recursive: true, force: true });
    }
  });

  test("refuses to replace the project root", async () => {
    await expect(buildSite({ root, outDir: root })).rejects.toThrow(
      "output directory must not be the project root",
    );
  });
});
