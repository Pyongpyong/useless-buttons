// Package the locally built library and demo as a standalone static site.
import { mkdir, readFile, writeFile, copyFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const html = await readFile(new URL("demo/index.html", root), "utf8");
const libraryImport = '"../dist/index.js"';
if (!html.includes(libraryImport)) {
  throw new Error("Demo library import changed; update build-demo.mjs accordingly.");
}
await mkdir(new URL("site/dist/", root), { recursive: true });
await copyFile(new URL("dist/index.js", root), new URL("site/dist/index.js", root));
await writeFile(new URL("site/index.html", root), html.replace(libraryImport, '"./dist/index.js"'));
await writeFile(new URL("site/vercel.json", root), JSON.stringify({
  $schema: "https://openapi.vercel.sh/vercel.json",
  framework: null,
  buildCommand: "",
  installCommand: "",
  outputDirectory: ".",
}, null, 2) + "\n");
console.log("Demo ready in site/ (deploy this directory).");
