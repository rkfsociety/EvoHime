import assert from "node:assert/strict";
import test from "node:test";
import { classifyDiffLine } from "./diff.ts";

test("classifies unified diff content lines", () => {
  assert.equal(classifyDiffLine("+added"), "diffAdded");
  assert.equal(classifyDiffLine("-removed"), "diffRemoved");
  assert.equal(classifyDiffLine("@@ -1 +1 @@"), "diffContext");
  assert.equal(classifyDiffLine(" unchanged"), "");
});

test("keeps unified diff file headers neutral", () => {
  assert.equal(classifyDiffLine("+++ b/src/lib.rs"), "");
  assert.equal(classifyDiffLine("--- a/src/lib.rs"), "");
});
