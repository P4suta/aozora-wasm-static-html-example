import AxeBuilder from "@axe-core/playwright";
import { chromium, firefox, type Browser, type Page, webkit } from "@playwright/test";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { extname, join, resolve, sep } from "node:path";

interface Arguments {
  readonly baseline: string;
  readonly candidate: string;
  readonly outDir: string;
  readonly approval?: string;
  readonly browser?: "chromium" | "firefox" | "webkit";
  readonly mode?: "child";
  readonly work?: string;
}

function argumentsFrom(values: ReadonlyArray<string>): Arguments {
  const options = new Map<string, string>();
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index];
    const value = values[index + 1];
    if (key === undefined || value === undefined || !key.startsWith("--")) {
      throw new Error("expected --baseline, --candidate and --out-dir options");
    }
    options.set(key, value);
  }
  const baseline = options.get("--baseline");
  const candidate = options.get("--candidate");
  const outDir = options.get("--out-dir");
  if (baseline === undefined || candidate === undefined || outDir === undefined) {
    throw new Error("--baseline, --candidate and --out-dir are required");
  }
  const approval = options.get("--approval");
  const browser = options.get("--browser");
  if (browser !== undefined && !["chromium", "firefox", "webkit"].includes(browser)) {
    throw new Error("--browser must be chromium, firefox or webkit");
  }
  const mode = options.get("--mode");
  if (mode !== undefined && mode !== "child") throw new Error("--mode is internal");
  const work = options.get("--work");
  if (work !== undefined && !/^\d{6}-[0-9a-f]{16}$/.test(work)) {
    throw new Error("--work must be an editionId");
  }
  return {
    baseline: resolve(baseline),
    candidate: resolve(candidate),
    outDir: resolve(outDir),
    ...(approval === undefined ? {} : { approval: resolve(approval) }),
    ...(browser === undefined ? {} : { browser: browser as "chromium" | "firefox" | "webkit" }),
    ...(mode === undefined ? {} : { mode: "child" as const }),
    ...(work === undefined ? {} : { work }),
  };
}

async function withTimeout<T>(
  label: string,
  promise: Promise<T>,
  milliseconds = 60_000,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} timed out after ${milliseconds}ms`)),
          milliseconds,
        );
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

const mimeTypes: Readonly<Record<string, string>> = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
};

async function freePort(): Promise<number> {
  return await new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (typeof address === "string" || address === null) {
        server.close();
        reject(new Error("failed to allocate a TCP port"));
        return;
      }
      server.close((error) => (error === undefined ? resolve(address.port) : reject(error)));
    });
  });
}

function serve(root: string, port: number): ReturnType<typeof Bun.serve> {
  return Bun.serve({
    port,
    async fetch(request) {
      const pathname = decodeURIComponent(new URL(request.url).pathname);
      const target = resolve(root, `.${pathname === "/" ? "/index.html" : pathname}`);
      if (target !== root && !target.startsWith(`${root}${sep}`))
        return new Response("forbidden", { status: 403 });
      try {
        if (!(await stat(target)).isFile()) return new Response("not found", { status: 404 });
        return new Response(Bun.file(target), {
          headers: { "content-type": mimeTypes[extname(target)] ?? "application/octet-stream" },
        });
      } catch {
        return new Response("not found", { status: 404 });
      }
    },
  });
}

function digest(value: Uint8Array | string): string {
  return createHash("sha256").update(value).digest("hex");
}

function merkle(leaves: ReadonlyArray<string>): string {
  if (leaves.length === 0) return digest("");
  let level = [...leaves].sort().map((leaf) => digest(leaf));
  while (level.length > 1) {
    const next: string[] = [];
    for (let index = 0; index < level.length; index += 2) {
      next.push(digest(`${level[index]}${level[index + 1] ?? level[index]}`));
    }
    level = next;
  }
  const root = level[0];
  if (root === undefined) throw new Error("Merkle root disappeared");
  return root;
}

async function fingerprint(page: Page): Promise<unknown> {
  return page.locator(".reader.aozora-notation").evaluate((reader) => {
    const rounded = (value: number): number => Math.round(value * 64) / 64;
    const rect = (value: DOMRect): ReadonlyArray<number> =>
      [value.x, value.y, value.width, value.height].map(rounded);
    return Array.from(reader.querySelectorAll("*"), (element) => {
      const style = getComputedStyle(element);
      const textRects: Array<ReadonlyArray<number>> = [];
      for (const node of Array.from(element.childNodes)) {
        if (node.nodeType === Node.TEXT_NODE && (node.textContent ?? "").length > 0) {
          const range = document.createRange();
          range.selectNodeContents(node);
          textRects.push(...Array.from(range.getClientRects(), rect));
        }
      }
      return {
        tag: element.tagName.toLowerCase(),
        class: element.getAttribute("class") ?? "",
        rect: rect(element.getBoundingClientRect()),
        scroll: [element.scrollWidth, element.scrollHeight],
        textRects,
        style: {
          display: style.display,
          position: style.position,
          writingMode: style.writingMode,
          fontFamily: style.fontFamily,
          fontSize: style.fontSize,
          fontWeight: style.fontWeight,
          lineHeight: style.lineHeight,
          letterSpacing: style.letterSpacing,
          whiteSpace: style.whiteSpace,
          overflowX: style.overflowX,
          rubyPosition: style.rubyPosition,
        },
      };
    });
  });
}

async function contactSheet(page: Page): Promise<Buffer> {
  const selectors = [
    ".reader > :first-child",
    ".reader > :last-child",
    ".reader ruby",
    ".reader .aozora-gaiji",
    ".reader [class*='container'], .reader [class*='indent']",
    ".reader [class*='bouten'], .reader em",
  ];
  await page.evaluate((values) => {
    document.querySelector("#aozora-lab-contact-sheet")?.remove();
    const sheet = document.createElement("aside");
    sheet.id = "aozora-lab-contact-sheet";
    sheet.className = "reader";
    sheet.style.cssText =
      "position:absolute;z-index:2147483647;left:0;top:0;width:1000px;padding:16px;background:white;color:#24211d;display:grid;grid-template-columns:1fr 1fr;gap:16px";
    for (const selector of values) {
      const source = document.querySelector(selector);
      if (!(source instanceof HTMLElement) || source.getClientRects().length === 0) continue;
      const crop = document.createElement("section");
      crop.style.cssText =
        "min-width:0;max-height:700px;overflow:hidden;border:1px solid #777;padding:8px;background:white";
      const label = document.createElement("p");
      label.textContent = selector;
      label.style.cssText = "margin:0 0 6px;font:12px system-ui;color:#222";
      crop.append(label, source.cloneNode(true));
      sheet.append(crop);
    }
    document.body.append(sheet);
  }, selectors);
  try {
    return await page.locator("#aozora-lab-contact-sheet").screenshot({ animations: "disabled" });
  } finally {
    await page.evaluate(() => document.querySelector("#aozora-lab-contact-sheet")?.remove());
  }
}

async function differenceImage(
  browser: Browser,
  baseline: Buffer,
  candidate: Buffer,
): Promise<Buffer> {
  const page = await browser.newPage({ viewport: { width: 1000, height: 800 } });
  try {
    await page.setContent(
      `<!doctype html><style>html,body{margin:0;background:white}canvas{display:block}</style><canvas></canvas>`,
    );
    await page.evaluate(
      async ({ baseline, candidate }) => {
        const load = (source: string): Promise<HTMLImageElement> =>
          new Promise((resolve, reject) => {
            const image = new Image();
            image.onload = () => resolve(image);
            image.onerror = () => reject(new Error("contact sheet image failed to load"));
            image.src = source;
          });
        const [before, after] = await Promise.all([load(baseline), load(candidate)]);
        const canvas = document.querySelector("canvas");
        if (!(canvas instanceof HTMLCanvasElement)) throw new Error("diff canvas is missing");
        canvas.width = Math.max(before.width, after.width);
        canvas.height = Math.max(before.height, after.height);
        const context = canvas.getContext("2d");
        if (context === null) throw new Error("diff canvas context is missing");
        context.drawImage(before, 0, 0);
        context.globalCompositeOperation = "difference";
        context.drawImage(after, 0, 0);
      },
      {
        baseline: `data:image/png;base64,${baseline.toString("base64")}`,
        candidate: `data:image/png;base64,${candidate.toString("base64")}`,
      },
    );
    return await page.locator("canvas").screenshot();
  } finally {
    await page.close();
  }
}

interface Difference {
  readonly editionId: string;
  readonly browser: string;
  readonly viewport: string;
  readonly layout: boolean;
  readonly pixels: boolean;
}

interface VisualReport {
  readonly schemaVersion: 1;
  readonly baselineMerkleRoot: string;
  readonly candidateMerkleRoot: string;
  readonly differences: ReadonlyArray<Difference>;
  readonly approved: boolean;
}

async function approvalMatches(
  path: string | undefined,
  baselineMerkleRoot: string,
  candidateMerkleRoot: string,
  differences: ReadonlyArray<Difference>,
): Promise<boolean> {
  if (differences.length === 0) return true;
  if (path === undefined) return false;
  const editions = [...new Set(differences.map(({ editionId }) => editionId))].sort();
  const approval = JSON.parse(await readFile(path, "utf8")) as unknown;
  return (
    typeof approval === "object" &&
    approval !== null &&
    Object.keys(approval).sort().join(",") ===
      "baselineMerkleRoot,candidateMerkleRoot,editions,issue,reason,schemaVersion" &&
    (approval as Record<string, unknown>)["schemaVersion"] === 1 &&
    typeof (approval as Record<string, unknown>)["reason"] === "string" &&
    typeof (approval as Record<string, unknown>)["issue"] === "string" &&
    (approval as Record<string, unknown>)["baselineMerkleRoot"] === baselineMerkleRoot &&
    (approval as Record<string, unknown>)["candidateMerkleRoot"] === candidateMerkleRoot &&
    JSON.stringify((approval as Record<string, unknown>)["editions"]) === JSON.stringify(editions)
  );
}

async function orchestrate(options: Arguments): Promise<void> {
  const reports: Array<{ browser: string; report: VisualReport }> = [];
  for (const browser of ["chromium", "firefox", "webkit"] as const) {
    const outDir = join(options.outDir, browser);
    const command = [
      process.execPath,
      import.meta.path,
      "--baseline",
      options.baseline,
      "--candidate",
      options.candidate,
      "--out-dir",
      outDir,
      "--browser",
      browser,
      "--mode",
      "child",
    ];
    if (options.work !== undefined) command.push("--work", options.work);
    const child = Bun.spawn({
      cmd: command,
      stdin: "ignore",
      stdout: "inherit",
      stderr: "inherit",
    });
    const exitCode = await child.exited;
    if (exitCode !== 0)
      throw new Error(`${browser} visual child failed with exit code ${exitCode}`);
    reports.push({
      browser,
      report: JSON.parse(await readFile(join(outDir, "report.json"), "utf8")) as VisualReport,
    });
  }
  const baselineMerkleRoot = merkle(
    reports.map(({ browser, report }) => `${browser}/${report.baselineMerkleRoot}`),
  );
  const candidateMerkleRoot = merkle(
    reports.map(({ browser, report }) => `${browser}/${report.candidateMerkleRoot}`),
  );
  const differences = reports.flatMap(({ report }) => report.differences);
  const approved = await approvalMatches(
    options.approval,
    baselineMerkleRoot,
    candidateMerkleRoot,
    differences,
  );
  await mkdir(options.outDir, { recursive: true });
  await writeFile(
    join(options.outDir, "report.json"),
    `${JSON.stringify({ schemaVersion: 1, baselineMerkleRoot, candidateMerkleRoot, differences, approved }, null, 2)}\n`,
  );
  if (!approved) {
    throw new Error(
      `${differences.length} visual difference(s) require an exact approval manifest`,
    );
  }
}

async function main(): Promise<void> {
  const options = argumentsFrom(process.argv.slice(2));
  if (options.browser === undefined) {
    await orchestrate(options);
    return;
  }
  let baselineWorks = (await readdir(join(options.baseline, "works")))
    .filter((file) => file.endsWith(".html"))
    .sort();
  let candidateWorks = (await readdir(join(options.candidate, "works")))
    .filter((file) => file.endsWith(".html"))
    .sort();
  if (JSON.stringify(baselineWorks) !== JSON.stringify(candidateWorks)) {
    throw new Error("baseline and candidate edition sets differ");
  }
  if (options.work !== undefined) {
    baselineWorks = baselineWorks.filter((file) => file === `${options.work}.html`);
    candidateWorks = candidateWorks.filter((file) => file === `${options.work}.html`);
    if (candidateWorks.length !== 1) throw new Error(`unknown editionId ${options.work}`);
  }
  const baselineServer = serve(options.baseline, await freePort());
  const candidateServer = serve(options.candidate, await freePort());
  const differences: Difference[] = [];
  const baselineLeaves: string[] = [];
  const candidateLeaves: string[] = [];
  try {
    for (const [browserName, browserType] of [
      ["chromium", chromium],
      ["firefox", firefox],
      ["webkit", webkit],
    ] as const) {
      if (options.browser !== undefined && options.browser !== browserName) continue;
      const browser = await browserType.launch();
      try {
        for (const viewport of [
          { name: "narrow", width: 320, height: 720 },
          { name: "wide", width: 1440, height: 900 },
        ]) {
          const context = await browser.newContext({ viewport });
          try {
            for (const filename of candidateWorks) {
              const before = await context.newPage();
              const after = await context.newPage();
              const editionId = filename.slice(0, -5);
              try {
                console.error(`visual ${browserName}/${viewport.name}/${editionId}`);
                await withTimeout(
                  `${browserName}/${viewport.name}/${editionId} navigation`,
                  Promise.all([
                    before.goto(`http://127.0.0.1:${baselineServer.port}/works/${filename}`, {
                      waitUntil: "networkidle",
                    }),
                    after.goto(`http://127.0.0.1:${candidateServer.port}/works/${filename}`, {
                      waitUntil: "networkidle",
                    }),
                  ]),
                );
                console.error(`visual ${browserName}/${viewport.name}/${editionId} loaded`);
                const overflow = await after.evaluate(
                  () =>
                    document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
                );
                if (overflow)
                  throw new Error(
                    `${editionId} has horizontal overflow in ${browserName}/${viewport.name}`,
                  );
                const [beforeLayout, afterLayout, beforeSheet, afterSheet] = await withTimeout(
                  `${browserName}/${viewport.name}/${editionId} capture`,
                  (async () => {
                    const beforeLayout = await fingerprint(before);
                    const afterLayout = await fingerprint(after);
                    const beforeSheet = await contactSheet(before);
                    const afterSheet = await contactSheet(after);
                    return [beforeLayout, afterLayout, beforeSheet, afterSheet] as const;
                  })(),
                );
                console.error(`visual ${browserName}/${viewport.name}/${editionId} captured`);
                const accessibility = await withTimeout(
                  `${browserName}/${viewport.name}/${editionId} axe`,
                  new AxeBuilder({ page: after }).analyze(),
                );
                if (accessibility.violations.length > 0) {
                  throw new Error(
                    `${editionId} has accessibility violations in ${browserName}/${viewport.name}: ${JSON.stringify(accessibility.violations.map(({ id }) => id))}`,
                  );
                }
                console.error(`visual ${browserName}/${viewport.name}/${editionId} axe`);
                const beforeFingerprint = JSON.stringify(beforeLayout);
                const afterFingerprint = JSON.stringify(afterLayout);
                const beforeDigest = digest(beforeSheet);
                const afterDigest = digest(afterSheet);
                baselineLeaves.push(
                  `${browserName}/${viewport.name}/${editionId}/${digest(beforeFingerprint)}/${beforeDigest}`,
                );
                candidateLeaves.push(
                  `${browserName}/${viewport.name}/${editionId}/${digest(afterFingerprint)}/${afterDigest}`,
                );
                const difference = {
                  editionId,
                  browser: browserName,
                  viewport: viewport.name,
                  layout: beforeFingerprint !== afterFingerprint,
                  pixels: beforeDigest !== afterDigest,
                };
                if (difference.layout || difference.pixels) {
                  differences.push(difference);
                  const directory = join(options.outDir, browserName, viewport.name);
                  await mkdir(directory, { recursive: true });
                  await Promise.all([
                    writeFile(join(directory, `${editionId}.baseline.png`), beforeSheet),
                    writeFile(join(directory, `${editionId}.candidate.png`), afterSheet),
                    writeFile(
                      join(directory, `${editionId}.diff.png`),
                      await differenceImage(browser, beforeSheet, afterSheet),
                    ),
                    writeFile(
                      join(directory, `${editionId}.baseline.layout.json`),
                      `${JSON.stringify(beforeLayout, null, 2)}\n`,
                    ),
                    writeFile(
                      join(directory, `${editionId}.candidate.layout.json`),
                      `${JSON.stringify(afterLayout, null, 2)}\n`,
                    ),
                  ]);
                }
              } finally {
                await Promise.all([before.close(), after.close()]);
              }
            }
          } finally {
            await context.close();
          }
        }
      } finally {
        await browser.close();
      }
    }
  } finally {
    baselineServer.stop(true);
    candidateServer.stop(true);
  }
  const baselineMerkleRoot = merkle(baselineLeaves);
  const candidateMerkleRoot = merkle(candidateLeaves);
  const approved = await approvalMatches(
    options.approval,
    baselineMerkleRoot,
    candidateMerkleRoot,
    differences,
  );
  await mkdir(options.outDir, { recursive: true });
  await writeFile(
    join(options.outDir, "report.json"),
    `${JSON.stringify({ schemaVersion: 1, baselineMerkleRoot, candidateMerkleRoot, differences, approved }, null, 2)}\n`,
  );
  if (!approved && options.mode !== "child") {
    throw new Error(
      `${differences.length} visual difference(s) require an exact approval manifest`,
    );
  }
}

await main();
