import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { compileFromFile } from "json-schema-to-typescript";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const schemaPath = join(
  root,
  "crates/protocol/schema/evohime.protocol.schema.json",
);
const outputPath = join(root, "frontend/web/src/protocol.generated.ts");

const banner = `/* eslint-disable */
/**
 * Generated from crates/protocol/schema/evohime.protocol.schema.json.
 * Do not edit by hand. Run: npm run generate:protocol
 */
`;

const serverEvent = await compileFromFile(schemaPath, {
  cwd: root,
  bannerComment: banner,
  style: {
    singleQuote: false,
  },
  additionalProperties: false,
});

const historyTypes = `
export type HistoryItem = {
  sequence: number;
  created_at: string;
  event: ServerEvent;
};

export type SessionBootstrap = {
  session_id: string;
  created_at: string;
  events: HistoryItem[];
};
`;

writeFileSync(outputPath, `${serverEvent}\n${historyTypes}`);
console.log(`Wrote ${outputPath}`);
