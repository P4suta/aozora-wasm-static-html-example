import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

interface Request {
  readonly protocolVersion: 1;
  readonly requestId: string;
  readonly operation: "render";
  readonly source: string;
}

function parseRequest(value: unknown): Request {
  if (
    typeof value !== "object" ||
    value === null ||
    Object.keys(value).sort().join(",") !== "operation,protocolVersion,requestId,source"
  ) {
    throw new Error("invalid request envelope");
  }
  const input = value as Record<string, unknown>;
  if (
    input["protocolVersion"] !== 1 ||
    input["operation"] !== "render" ||
    typeof input["requestId"] !== "string" ||
    input["requestId"] === "" ||
    typeof input["source"] !== "string"
  ) {
    throw new Error("invalid request fields");
  }
  return input as unknown as Request;
}

const packageArgument = process.argv[2];
if (packageArgument === undefined) {
  throw new Error("usage: release-wasm.ts <extracted-npm-package>");
}
const packageRoot = resolve(packageArgument);
const api = await import(pathToFileURL(join(packageRoot, "aozora_wasm.js")).href);
const wasm = await readFile(join(packageRoot, "aozora_wasm_bg.wasm"));
await api.default({ module_or_path: wasm });

const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
for await (const line of lines) {
  let requestId = "invalid";
  try {
    const input = parseRequest(JSON.parse(line) as unknown);
    requestId = input.requestId;
    const document = new api.Document(input.source);
    try {
      process.stdout.write(
        `${JSON.stringify({
          protocolVersion: 1,
          requestId,
          ok: true,
          result: {
            version: api.version(),
            schemaVersion: api.schemaVersion(),
            html: document.toHtml(),
            diagnostics: document.diagnostics(),
            gaiji: document.gaiji(),
            nodes: document.nodes(),
            pairs: document.pairs(),
            containerPairs: document.containerPairs(),
            source: document.toSource(),
          },
        })}\n`,
      );
    } finally {
      document.free();
    }
  } catch (error) {
    process.stdout.write(
      `${JSON.stringify({
        protocolVersion: 1,
        requestId,
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      })}\n`,
    );
  }
}
