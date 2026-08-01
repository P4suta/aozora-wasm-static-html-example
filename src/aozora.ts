import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Diagnostic, GaijiResolution } from "aozora-wasm";
import { parserGeneratedHtml, type SafeHtml } from "./html.ts";

type AozoraApi = typeof import("aozora-wasm");

export interface LoadedAozora {
  readonly api: AozoraApi;
  readonly version: string;
}

interface DocumentHandle {
  diagnostics(): ReadonlyArray<Diagnostic>;
  gaiji(): ReadonlyArray<GaijiResolution>;
  toHtml(): string;
  free(): void;
}

export interface DocumentApi {
  readonly Document: new (source: string) => DocumentHandle;
}

export interface RenderedBody {
  readonly html: SafeHtml;
  readonly diagnosticCount: number;
  readonly gaijiCount: number;
}

let aozoraPromise: Promise<LoadedAozora> | undefined;

async function initializeAozora(): Promise<LoadedAozora> {
  const api = await import("aozora-wasm");
  const entryUrl = import.meta.resolve("aozora-wasm");
  const packageRoot = dirname(fileURLToPath(entryUrl));
  const wasmBytes = await readFile(join(packageRoot, "aozora_wasm_bg.wasm"));
  const module = Uint8Array.from(wasmBytes).buffer;
  api.initSync({ module });
  api.prewarm();
  return { api, version: api.version() };
}

export function loadAozora(): Promise<LoadedAozora> {
  aozoraPromise ??= initializeAozora();
  return aozoraPromise;
}

export function renderAozoraBody(api: DocumentApi, source: string, label: string): RenderedBody {
  const document = new api.Document(source);
  try {
    const diagnostics = document.diagnostics();
    if (diagnostics.length !== 0) {
      throw new Error(`${label} produced diagnostics: ${JSON.stringify(diagnostics)}`);
    }
    return {
      html: parserGeneratedHtml(document.toHtml()),
      diagnosticCount: diagnostics.length,
      gaijiCount: document.gaiji().length,
    };
  } finally {
    document.free();
  }
}
