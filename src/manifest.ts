import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { ZodType } from "zod";
import {
  type Manifest,
  manifestSchema,
  type NotationCssProvenance,
  notationCssProvenanceSchema,
} from "./model.ts";
import { decodeUtf8 } from "./utf8.ts";

function parseJson<T>(bytes: Uint8Array, schema: ZodType<T>, label: string): T {
  const source = decodeUtf8(bytes, label);
  let input: unknown;
  try {
    input = JSON.parse(source);
  } catch (error) {
    throw new Error(`${label} is not valid JSON`, { cause: error });
  }
  return schema.parse(input);
}

export function parseManifest(bytes: Uint8Array, label = "works.json"): Manifest {
  return parseJson(bytes, manifestSchema, label);
}

export function parseNotationCssProvenance(
  bytes: Uint8Array,
  label = "vendor/aozora-notation.json",
): NotationCssProvenance {
  return parseJson(bytes, notationCssProvenanceSchema, label);
}

export async function loadBuildInputs(root: string): Promise<{
  readonly manifest: Manifest;
  readonly notationCssProvenance: NotationCssProvenance;
  readonly notationCss: Uint8Array;
}> {
  const [manifestBytes, provenanceBytes, notationCss] = await Promise.all([
    readFile(join(root, "works.json")),
    readFile(join(root, "vendor", "aozora-notation.json")),
    readFile(join(root, "vendor", "aozora-notation.css")),
  ]);

  return {
    manifest: parseManifest(manifestBytes),
    notationCssProvenance: parseNotationCssProvenance(provenanceBytes),
    notationCss,
  };
}
