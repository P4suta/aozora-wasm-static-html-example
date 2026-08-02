import { createInterface } from "node:readline";
import { loadAozora } from "../aozora.ts";

interface Request {
  readonly protocolVersion: 1;
  readonly requestId: string;
  readonly operation: "render";
  readonly source: string;
}

function request(value: unknown): Request {
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

const { api, version } = await loadAozora();
const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY });
for await (const line of lines) {
  let requestId = "invalid";
  try {
    const input = request(JSON.parse(line) as unknown);
    requestId = input.requestId;
    const document = new api.Document(input.source);
    try {
      process.stdout.write(
        `${JSON.stringify({
          protocolVersion: 1,
          requestId,
          ok: true,
          result: {
            version,
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
