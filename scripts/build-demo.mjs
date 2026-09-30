// Package the locally built library and demo as a standalone static site.
import { mkdir, readFile, readdir, writeFile, copyFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const libraryImport = '"../dist/index.js"';
await mkdir(new URL("site/dist/", root), { recursive: true });
await copyFile(new URL("dist/index.js", root), new URL("site/dist/index.js", root));
// Every page in demo/ (the demo itself plus its example pages) ships
// with its library import pointed at the copied bundle.
const pages = (await readdir(new URL("demo/", root))).filter((f) => f.endsWith(".html"));
for (const page of pages) {
  const html = await readFile(new URL(`demo/${page}`, root), "utf8");
  if (!html.includes(libraryImport)) {
    throw new Error(`demo/${page} doesn't import ${libraryImport}; update build-demo.mjs accordingly.`);
  }
  await writeFile(new URL(`site/${page}`, root), html.replace(libraryImport, '"./dist/index.js"'));
}
await writeFile(new URL("site/vercel.json", root), JSON.stringify({
  $schema: "https://openapi.vercel.sh/vercel.json",
  framework: null,
  buildCommand: "",
  installCommand: "",
  outputDirectory: ".",
}, null, 2) + "\n");
console.log("Demo ready in site/ (deploy this directory).");
