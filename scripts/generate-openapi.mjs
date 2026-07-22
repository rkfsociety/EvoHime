import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = readFileSync(join(root, "crates/server/src/routes.rs"), "utf8");
const routes = [];
const routePattern = /\.route\(\s*"([^"]+)"\s*,([\s\S]*?)(?=\n\s*\.route\(|\n\s*\.layer\(|\n\s*\.with_state\()/g;

for (const match of source.matchAll(routePattern)) {
  for (const method of match[2].matchAll(/\b(get|post|put|patch|delete)\s*\(/g)) {
    routes.push({ path: match[1], method: method[1] });
  }
}
if (routes.length === 0) throw new Error("No Axum routes found");

const operationId = (method, path) => `${method}_${path.replace(/^\//, "").replace(/[^a-zA-Z0-9]+/g, "_").replace(/_$/, "") || "root"}`;
const paths = {};
for (const { path, method } of routes) {
  paths[path] ??= {};
  paths[path][method] = {
    operationId: operationId(method, path),
    responses: { "200": { description: "Successful response" } },
  };
}

const document = {
  openapi: "3.0.3",
  info: { title: "EvoHime HTTP API", version: "0.1.0", description: "Generated from crates/server/src/routes.rs." },
  servers: [{ url: "http://localhost:3000" }],
  paths,
  "x-evohime-source": "crates/server/src/routes.rs",
};
writeFileSync(join(root, "docs/openapi.json"), `${JSON.stringify(document, null, 2)}\n`);

const pathEntries = Object.entries(paths).map(([path, methods]) =>
  `  ${JSON.stringify(path)}: ${JSON.stringify(Object.keys(methods))} as const,`,
).join("\n");
writeFileSync(join(root, "frontend/web/src/api/generated.ts"), `/* eslint-disable */
/**
 * Generated from crates/server/src/routes.rs.
 * Do not edit by hand. Run: npm run generate:openapi
 */

export const openApiPaths = {
${pathEntries}
} as const;

export type OpenApiPath = keyof typeof openApiPaths;
export type OpenApiMethod<Path extends OpenApiPath> = (typeof openApiPaths)[Path][number];
`);
console.log(`Generated ${routes.length} operations`);
