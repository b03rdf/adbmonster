import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../src/lib/taskState.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } });
const { isTaskActive, reconcileTasks, visibleTasks } = await import("data:text/javascript;base64," + Buffer.from(outputText).toString("base64"));

test("late polling cannot overwrite a newer cancellation or clear-history event", () => {
  const current = { revision: 12, tasks: [{ id: "a", status: "cancelled" }] };
  const stale = { revision: 11, tasks: [{ id: "a", status: "running" }] };
  assert.equal(reconcileTasks(current, stale), current);
  const cleared = { revision: 13, tasks: [] };
  assert.equal(reconcileTasks(current, cleared), cleared);
  assert.equal(reconcileTasks(cleared, current), cleared);
});

test("device filtering preserves task ownership and includes cancelling tasks", () => {
  const a = { id: "a", deviceId: "one", status: "cancelling" };
  const b = { id: "b", deviceId: "two", status: "running" };
  const done = { id: "c", deviceId: "one", status: "completed" };
  assert.deepEqual(visibleTasks([a, b, done], "one", true), [a]);
  assert.deepEqual(visibleTasks([a, b, done], "two"), [b]);
  assert.deepEqual(visibleTasks([a, b, done], undefined, true), [a, b]);
  assert.equal(a.deviceId, "one");
});

test("only starting, running and cancelling tasks reserve active slots", () => {
  for (const status of ["starting", "running", "cancelling"]) assert.equal(isTaskActive({ status }), true);
  for (const status of ["completed", "cancelled", "failed", "interrupted"]) assert.equal(isTaskActive({ status }), false);
});
