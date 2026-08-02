import { describe, expect, test } from "bun:test";
import type { Diagnostic, GaijiResolution } from "aozora-wasm";
import { loadAozora, renderAozoraBody, type DocumentApi } from "../src/aozora.ts";

class CleanDocument {
  static freeCount = 0;

  diagnostics(): ReadonlyArray<Diagnostic> {
    return [];
  }

  gaiji(): ReadonlyArray<GaijiResolution> {
    return [
      {
        span: { start: 0, end: 1 },
        description: "fixture",
        resolved: "字",
        codepoint: 0x5b57,
      },
    ];
  }

  toHtml(): string {
    return "<p>本文</p>";
  }

  free(): void {
    CleanDocument.freeCount += 1;
  }
}

class DiagnosticDocument extends CleanDocument {
  override diagnostics(): ReadonlyArray<Diagnostic> {
    return [
      {
        kind: "fixture",
        severity: "warning",
        source: "source",
        span: { start: 0, end: 1 },
      },
    ];
  }
}

class ThrowingDocument extends CleanDocument {
  override toHtml(): string {
    throw new Error("renderer failed");
  }
}

describe("aozora WASM boundary", () => {
  test("initializes once and reads the version from the WASM API", async () => {
    const first = await loadAozora();
    const second = await loadAozora();
    expect(first).toBe(second);
    expect(first.version).toBe("0.5.0");
  });

  test("renders diagnostic-free markup and always frees the document", () => {
    CleanDocument.freeCount = 0;
    const rendered = renderAozoraBody(
      { Document: CleanDocument } satisfies DocumentApi,
      "本文",
      "fixture",
    );
    expect(String(rendered.html)).toBe("<p>本文</p>");
    expect(rendered.diagnosticCount).toBe(0);
    expect(rendered.gaijiCount).toBe(1);
    expect(CleanDocument.freeCount).toBe(1);
  });

  test("rejects diagnostics and frees the document", () => {
    CleanDocument.freeCount = 0;
    expect(() =>
      renderAozoraBody({ Document: DiagnosticDocument } satisfies DocumentApi, "本文", "fixture"),
    ).toThrow("fixture produced diagnostics");
    expect(CleanDocument.freeCount).toBe(1);
  });

  test("frees the document when rendering throws", () => {
    CleanDocument.freeCount = 0;
    expect(() =>
      renderAozoraBody({ Document: ThrowingDocument } satisfies DocumentApi, "本文", "fixture"),
    ).toThrow("renderer failed");
    expect(CleanDocument.freeCount).toBe(1);
  });
});
