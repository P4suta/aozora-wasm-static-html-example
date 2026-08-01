import { z } from "zod";

const sha256Schema = z.string().regex(/^[0-9a-f]{64}$/);
const isoDateSchema = z.iso.date();
const trimmedTextSchema = z
  .string()
  .min(1)
  .refine((value) => value.trim() === value, "must not have surrounding whitespace");

const aozoraUrlSchema = trimmedTextSchema.superRefine((value, context) => {
  if (!URL.canParse(value)) {
    context.addIssue({ code: "custom", message: "must be a valid URL" });
    return;
  }
  const url = new URL(value);
  if (
    url.protocol !== "https:" ||
    url.hostname !== "www.aozora.gr.jp" ||
    url.username !== "" ||
    url.password !== "" ||
    url.hash !== ""
  ) {
    context.addIssue({ code: "custom", message: "must be an HTTPS www.aozora.gr.jp URL" });
  }
});

const contributorSchema = z
  .object({
    role: trimmedTextSchema,
    name: trimmedTextSchema,
  })
  .strict();

const workSchema = z
  .object({
    id: z.string().regex(/^\d{6}$/),
    title: trimmedTextSchema,
    reading: trimmedTextSchema,
    copyright: z.literal("なし"),
    contributors: z.array(contributorSchema).min(1),
    cardUrl: aozoraUrlSchema,
    archiveUrl: aozoraUrlSchema,
    archiveFilename: z
      .string()
      .regex(/^[A-Za-z0-9][A-Za-z0-9._-]*\.txt$/)
      .refine((value) => value !== "." && value !== ".."),
    archiveSha256: sha256Schema,
    sourceSha256: sha256Schema,
  })
  .strict();

const metadataSourceSchema = z
  .object({
    url: aozoraUrlSchema,
    snapshotDate: isoDateSchema,
    retrievedDate: isoDateSchema,
    sha256: sha256Schema,
  })
  .strict();

export const manifestSchema = z
  .object({
    metadataSource: metadataSourceSchema,
    works: z.array(workSchema).length(10),
  })
  .strict()
  .superRefine(({ metadataSource, works }, context) => {
    const ids = new Set<string>();
    for (const [index, work] of works.entries()) {
      if (ids.has(work.id)) {
        context.addIssue({
          code: "custom",
          message: `duplicate work id: ${work.id}`,
          path: ["works", index, "id"],
        });
      }
      ids.add(work.id);
    }

    if (metadataSource.retrievedDate < metadataSource.snapshotDate) {
      context.addIssue({
        code: "custom",
        message: "retrievedDate must not precede snapshotDate",
        path: ["metadataSource", "retrievedDate"],
      });
    }
  });

export const notationCssProvenanceSchema = z
  .object({
    source: trimmedTextSchema,
    commit: z.string().regex(/^[0-9a-f]{40}$/),
    sha256: sha256Schema,
    license: z.literal("Apache-2.0 OR MIT"),
  })
  .strict()
  .superRefine(({ source, commit }, context) => {
    if (!URL.canParse(source)) {
      context.addIssue({
        code: "custom",
        message: "source must be a valid URL",
        path: ["source"],
      });
      return;
    }
    const url = new URL(source);
    if (
      url.protocol !== "https:" ||
      url.hostname !== "github.com" ||
      url.pathname !== `/P4suta/aozora/blob/${commit}/crates/aozora/assets/aozora-notation.css` ||
      url.search !== "" ||
      url.hash !== ""
    ) {
      context.addIssue({
        code: "custom",
        message: "source must identify the vendored aozora notation stylesheet at commit",
        path: ["source"],
      });
    }
  });

export type Work = z.infer<typeof workSchema>;
type MetadataSource = z.infer<typeof metadataSourceSchema>;
export type Manifest = z.infer<typeof manifestSchema>;
export type NotationCssProvenance = z.infer<typeof notationCssProvenanceSchema>;

export interface BuildWorkReport {
  readonly id: string;
  readonly title: string;
  readonly url: string;
  readonly sourceSha256: string;
  readonly outputSha256: string;
  readonly diagnosticCount: number;
  readonly gaijiCount: number;
}

export interface BuildReport {
  readonly generator: {
    readonly package: "aozora-wasm";
    readonly version: string;
  };
  readonly metadataSource: MetadataSource;
  readonly notationCss: NotationCssProvenance;
  readonly works: ReadonlyArray<BuildWorkReport>;
}
