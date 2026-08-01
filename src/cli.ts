import { resolve } from "node:path";
import { buildSite } from "./build.ts";

await buildSite({ root: resolve(process.cwd()) });
