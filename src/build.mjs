import { createHash } from "node:crypto";
import {
  copyFile,
  cp,
  mkdir,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { splitAozoraFile } from "./envelope.mjs";
import { indexPage, workPage } from "./html.mjs";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const HASH_PATTERN = /^[0-9a-f]{64}$/;
let aozoraApi;

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function validateManifest(manifest) {
  if (!Array.isArray(manifest.works) || manifest.works.length !== 10) {
    throw new Error("manifest must contain exactly ten works");
  }
  const ids = new Set();
  for (const work of manifest.works) {
    if (!/^\d{6}$/.test(work.id)) throw new Error(`invalid work id: ${work.id}`);
    if (ids.has(work.id)) throw new Error(`duplicate work id: ${work.id}`);
    ids.add(work.id);
    if (work.copyright !== "なし") throw new Error(`${work.id} is not rights-expired`);
    if (!Array.isArray(work.contributors) || work.contributors.length === 0) {
      throw new Error(`${work.id} has no contributors`);
    }
    if (!HASH_PATTERN.test(work.archiveSha256) || !HASH_PATTERN.test(work.sourceSha256)) {
      throw new Error(`${work.id} has an invalid source hash`);
    }
    for (const key of ["cardUrl", "archiveUrl"]) {
      const url = new URL(work[key]);
      if (url.protocol !== "https:" || url.hostname !== "www.aozora.gr.jp") {
        throw new Error(`${work.id} has an invalid ${key}`);
      }
    }
  }
}

async function loadAozora() {
  if (aozoraApi) return aozoraApi;
  const entryUrl = import.meta.resolve("aozora-wasm");
  const packageRoot = dirname(fileURLToPath(entryUrl));
  const api = await import("aozora-wasm");
  const module = await readFile(join(packageRoot, "aozora_wasm_bg.wasm"));
  api.initSync({ module });
  const packageJson = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  aozoraApi = { api, version: packageJson.version };
  return aozoraApi;
}

export async function buildSite({ root = ROOT, outDir = join(root, "dist") } = {}) {
  const manifest = JSON.parse(await readFile(join(root, "works.json"), "utf8"));
  validateManifest(manifest);
  const { api, version } = await loadAozora();

  await rm(outDir, { recursive: true, force: true });
  await mkdir(join(outDir, "styles"), { recursive: true });
  await mkdir(join(outDir, "sources"), { recursive: true });
  await copyFile(join(root, "src", "site.css"), join(outDir, "styles", "site.css"));
  await copyFile(
    join(root, "vendor", "aozora-notation.css"),
    join(outDir, "styles", "aozora-notation.css"),
  );

  const report = {
    generator: { package: "aozora-wasm", version },
    metadataSource: manifest.metadataSource,
    works: [],
  };

  for (const [index, work] of manifest.works.entries()) {
    const sourcePath = join(root, "sources", `${work.id}.txt`);
    const sourceBytes = await readFile(sourcePath);
    const actualSourceHash = sha256(sourceBytes);
    if (actualSourceHash !== work.sourceSha256) {
      throw new Error(`${work.id} source hash mismatch: ${actualSourceHash}`);
    }
    const source = sourceBytes.toString("utf8");
    if (source.includes("\uFFFD")) throw new Error(`${work.id} contains a decode replacement`);
    const envelope = splitAozoraFile(source);
    const document = new api.Document(envelope.body);
    let diagnostics;
    let html;
    let gaijiCount;
    try {
      diagnostics = document.diagnostics();
      if (diagnostics.length !== 0) {
        throw new Error(`${work.id} produced diagnostics: ${JSON.stringify(diagnostics)}`);
      }
      html = document.toHtml();
      gaijiCount = document.gaiji().length;
    } finally {
      document.free();
    }
    const page = workPage({
      work,
      html,
      bibliography: envelope.bibliography,
      version,
      previous: manifest.works[index - 1],
      next: manifest.works[index + 1],
    });
    const filename = `${work.id}.utf8.html`;
    await writeFile(join(outDir, filename), page);
    await copyFile(sourcePath, join(outDir, "sources", `${work.id}.txt`));
    report.works.push({
      id: work.id,
      title: work.title,
      url: `./${filename}`,
      sourceSha256: actualSourceHash,
      outputSha256: sha256(page),
      diagnosticCount: diagnostics.length,
      gaijiCount,
    });
  }

  await writeFile(
    join(outDir, "index.html"),
    indexPage({ works: manifest.works, version }),
  );
  await writeFile(join(outDir, "build-report.json"), `${JSON.stringify(report, null, 2)}\n`);
  await writeFile(join(outDir, ".nojekyll"), "");
  await cp(join(root, "LICENSE-APACHE"), join(outDir, "LICENSE-APACHE"));
  await cp(join(root, "LICENSE-MIT"), join(outDir, "LICENSE-MIT"));
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await buildSite();
}
