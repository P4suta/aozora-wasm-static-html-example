import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { escapeHtml, indexPage, parserGeneratedHtml, workPage } from "../src/html.ts";
import { parseManifest } from "../src/manifest.ts";
import type { Work } from "../src/model.ts";

const root = fileURLToPath(new URL("..", import.meta.url));

async function fixtureWork(): Promise<Work> {
  return parseManifest(await readFile(`${root}/works.json`)).works[0] as Work;
}

describe("HTML composition", () => {
  test("escapes all five HTML-sensitive characters", () => {
    expect(String(escapeHtml(`<&>"'`))).toBe("&lt;&amp;&gt;&quot;&#39;");
  });

  test("keeps parser markup while escaping metadata and bibliography", async () => {
    const base = await fixtureWork();
    const work: Work = {
      ...base,
      title: `<script>alert("title")</script>`,
      reading: `&<reading>`,
      contributors: [{ role: `"><img src=x>`, name: `O'Brien & Co.` }],
      cardUrl: `https://www.aozora.gr.jp/" onmouseover="alert(1)`,
    };
    const page = workPage({
      work,
      semanticHtml: parserGeneratedHtml("<p><ruby>青空<rt>あおぞら</rt></ruby></p>"),
      bibliography: `底本：<script>alert("book")</script>`,
      version: `0.5.0"><script>alert("version")</script>`,
    });

    expect(page).toContain("<ruby>青空<rt>あおぞら</rt></ruby>");
    expect(page).not.toContain("<script>");
    expect(page).toContain("&lt;script&gt;alert(&quot;title&quot;)&lt;/script&gt;");
    expect(page).toContain("O&#39;Brien &amp; Co.");
    expect(page).toContain("&quot; onmouseover=&quot;alert(1)");
  });

  test("loads canonical CSS before the consumer override and renders terminal navigation", async () => {
    const work = await fixtureWork();
    const page = workPage({
      work,
      semanticHtml: parserGeneratedHtml("<p>本文</p>"),
      bibliography: "底本：本",
      version: "0.5.0",
    });
    expect(page.indexOf("aozora-notation.css")).toBeLessThan(page.indexOf("site.css"));
    expect(page.match(/aria-hidden="true"/g)).toHaveLength(3);
  });

  test("renders a featured first card and escaped index metadata", async () => {
    const work = await fixtureWork();
    const page = indexPage({
      works: [work, { ...work, id: "999999", title: "二作目 & 続" }],
      version: "0.5.0<&",
    });
    expect(page.match(/class="work-card featured"/g)).toHaveLength(1);
    expect(page).toContain("二作目 &amp; 続");
    expect(page).toContain("aozora-wasm@0.5.0&lt;&amp;");
    expect(page.indexOf("aozora-notation.css")).toBeLessThan(page.indexOf("site.css"));
  });
});
