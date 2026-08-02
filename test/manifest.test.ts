import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { parseManifest, parseNotationCssProvenance } from "../src/manifest.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const encoder = new TextEncoder();

async function manifestInput(): Promise<Record<string, unknown>> {
  return JSON.parse(await readFile(`${root}/works.json`, "utf8")) as Record<string, unknown>;
}

describe("manifest schemas", () => {
  test("accepts the pinned manifest and stylesheet provenance", async () => {
    const manifest = parseManifest(await readFile(`${root}/works.json`));
    const provenance = parseNotationCssProvenance(
      await readFile(`${root}/vendor/aozora-notation.json`),
    );
    expect(manifest.works).toHaveLength(10);
    expect(new Set(manifest.works.map(({ id }) => id)).size).toBe(10);
    expect(provenance.source).toContain(`/blob/${provenance.commit}/`);
  });

  test("rejects unknown fields and malformed JSON", async () => {
    const input = await manifestInput();
    input["extra"] = true;
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow();
    expect(() => parseManifest(encoder.encode("{"))).toThrow("works.json is not valid JSON");
  });

  test("rejects duplicate work IDs", async () => {
    const input = await manifestInput();
    const works = input["works"] as Array<Record<string, unknown>>;
    const first = works[0];
    const second = works[1];
    expect(first).toBeDefined();
    expect(second).toBeDefined();
    if (first === undefined || second === undefined) throw new Error("fixture is incomplete");
    second["id"] = first["id"];
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow("duplicate work id");
  });

  test("rejects unsafe source fields and reversed dates", async () => {
    const input = await manifestInput();
    const metadata = input["metadataSource"] as Record<string, unknown>;
    metadata["retrievedDate"] = "2026-07-31";
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow(
      "retrievedDate must not precede snapshotDate",
    );

    metadata["retrievedDate"] = "2026-08-02";
    const works = input["works"] as Array<Record<string, unknown>>;
    const first = works[0];
    if (first === undefined) throw new Error("fixture is incomplete");
    first["cardUrl"] = "https://example.com/cards/5";
    first["archiveFilename"] = "../source.txt";
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow();

    first["cardUrl"] = "not a URL";
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow(
      "must be a valid URL",
    );

    first["cardUrl"] = "https://www.aozora.gr.jp/cards/000005/card5.html";
    metadata["snapshotDate"] = "2026-99-99";
    expect(() => parseManifest(encoder.encode(JSON.stringify(input)))).toThrow();
  });

  test("ties stylesheet provenance to its commit and canonical path", async () => {
    const input = JSON.parse(
      await readFile(`${root}/vendor/aozora-notation.json`, "utf8"),
    ) as Record<string, unknown>;
    input["commit"] = "0".repeat(40);
    expect(() => parseNotationCssProvenance(encoder.encode(JSON.stringify(input)))).toThrow(
      "source must identify the vendored aozora notation stylesheet at commit",
    );

    input["source"] = "not a URL";
    expect(() => parseNotationCssProvenance(encoder.encode(JSON.stringify(input)))).toThrow(
      "source must be a valid URL",
    );
  });

  test("rejects malformed UTF-8 metadata", () => {
    expect(() => parseManifest(new Uint8Array([0xc3, 0x28]), "fixture.json")).toThrow(
      "fixture.json is not valid UTF-8",
    );
  });
});
