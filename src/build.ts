import { copyFile, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { renderAozoraBody, loadAozora } from "./aozora.ts";
import { splitAozoraFile } from "./envelope.ts";
import { sha256, verifySha256 } from "./hash.ts";
import { indexPage, workPage } from "./html.ts";
import { loadBuildInputs } from "./manifest.ts";
import type { BuildReport, BuildWorkReport, Manifest, NotationCssProvenance } from "./model.ts";
import { decodeUtf8 } from "./utf8.ts";

export interface BuildOptions {
  readonly root: string;
  readonly outDir?: string;
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error && error.code === code;
}

async function replaceDirectory(staging: string, target: string): Promise<void> {
  const parent = dirname(target);
  const backupContainer = await mkdtemp(join(parent, `.${basename(target)}.backup-`));
  const backup = join(backupContainer, "previous");
  let previousMoved = false;
  try {
    try {
      await rename(target, backup);
      previousMoved = true;
    } catch (error) {
      if (!isFileSystemError(error, "ENOENT")) throw error;
    }

    try {
      await rename(staging, target);
    } catch (error) {
      if (previousMoved) await rename(backup, target);
      throw error;
    }
  } finally {
    await rm(backupContainer, { force: true, recursive: true });
  }
}

async function writeStaticFiles(input: {
  readonly root: string;
  readonly staging: string;
  readonly notationCss: Uint8Array;
}): Promise<void> {
  await Promise.all([
    mkdir(join(input.staging, "styles"), { recursive: true }),
    mkdir(join(input.staging, "sources"), { recursive: true }),
  ]);
  await Promise.all([
    copyFile(join(input.root, "src", "site.css"), join(input.staging, "styles", "site.css")),
    writeFile(join(input.staging, "styles", "aozora-notation.css"), input.notationCss),
    copyFile(join(input.root, "LICENSE-APACHE"), join(input.staging, "LICENSE-APACHE")),
    copyFile(join(input.root, "LICENSE-MIT"), join(input.staging, "LICENSE-MIT")),
    writeFile(join(input.staging, ".nojekyll"), ""),
  ]);
}

async function buildWorks(input: {
  readonly root: string;
  readonly staging: string;
  readonly manifest: Manifest;
  readonly version: string;
  readonly api: Awaited<ReturnType<typeof loadAozora>>["api"];
}): Promise<ReadonlyArray<BuildWorkReport>> {
  const reports: BuildWorkReport[] = [];
  for (const [index, work] of input.manifest.works.entries()) {
    const sourcePath = join(input.root, "sources", `${work.id}.txt`);
    const sourceBytes = await readFile(sourcePath);
    const sourceSha256 = verifySha256(sourceBytes, work.sourceSha256, `${work.id} source`);
    const source = decodeUtf8(sourceBytes, `${work.id} source`);
    const envelope = splitAozoraFile(source);
    const rendered = renderAozoraBody(input.api, envelope.body, work.id);
    const adjacentWorks = {
      ...(input.manifest.works[index - 1] === undefined
        ? {}
        : { previous: input.manifest.works[index - 1] }),
      ...(input.manifest.works[index + 1] === undefined
        ? {}
        : { next: input.manifest.works[index + 1] }),
    };
    const page = workPage({
      work,
      semanticHtml: rendered.html,
      bibliography: envelope.bibliography,
      version: input.version,
      ...adjacentWorks,
    });
    const filename = `${work.id}.utf8.html`;
    await Promise.all([
      writeFile(join(input.staging, filename), page),
      copyFile(sourcePath, join(input.staging, "sources", `${work.id}.txt`)),
    ]);
    reports.push({
      id: work.id,
      title: work.title,
      url: `./${filename}`,
      sourceSha256,
      outputSha256: sha256(page),
      diagnosticCount: rendered.diagnosticCount,
      gaijiCount: rendered.gaijiCount,
    });
  }
  return reports;
}

async function buildInto(input: {
  readonly root: string;
  readonly staging: string;
  readonly manifest: Manifest;
  readonly notationCssProvenance: NotationCssProvenance;
  readonly notationCss: Uint8Array;
}): Promise<BuildReport> {
  const { api, version } = await loadAozora();
  await writeStaticFiles(input);
  const works = await buildWorks({
    root: input.root,
    staging: input.staging,
    manifest: input.manifest,
    version,
    api,
  });
  const report: BuildReport = {
    generator: { package: "aozora-wasm", version },
    metadataSource: input.manifest.metadataSource,
    notationCss: input.notationCssProvenance,
    works,
  };
  await Promise.all([
    writeFile(
      join(input.staging, "index.html"),
      indexPage({ works: input.manifest.works, version }),
    ),
    writeFile(join(input.staging, "build-report.json"), `${JSON.stringify(report, null, 2)}\n`),
  ]);
  return report;
}

export async function buildSite(options: BuildOptions): Promise<BuildReport> {
  const root = resolve(options.root);
  const outDir = resolve(options.outDir ?? join(root, "dist"));
  if (outDir === root) throw new Error("output directory must not be the project root");

  const { manifest, notationCssProvenance, notationCss } = await loadBuildInputs(root);
  verifySha256(notationCss, notationCssProvenance.sha256, "vendor/aozora-notation.css");

  await mkdir(dirname(outDir), { recursive: true });
  const staging = await mkdtemp(join(dirname(outDir), `.${basename(outDir)}.stage-`));
  try {
    const report = await buildInto({
      root,
      staging,
      manifest,
      notationCssProvenance,
      notationCss,
    });
    await replaceDirectory(staging, outDir);
    return report;
  } finally {
    await rm(staging, { force: true, recursive: true });
  }
}
