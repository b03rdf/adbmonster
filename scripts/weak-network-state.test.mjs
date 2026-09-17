import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

// Run the actual frontend state transition without a browser or Tauri runtime.
const source = await readFile(new URL("../src/lib/weakNetworkState.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
});
const { reconcileWeakNetworkStatus: reconcile } = await import(
  "data:text/javascript;base64," + Buffer.from(outputText).toString("base64")
);
const ready = {
  supported: true, helperInstalled: true, helperUpdateRequired: false,
  vpnAuthorized: true, helperRunning: false, helperVersion: "1.0.1",
  activeTargetPackage: null, expiresAt: null, message: "ready",
};
const started = {
  active: true, deviceId: "device-a", targetPackage: "com.example.game",
  expiresAt: "2030-01-01T00:00:00Z", message: "running",
};
const stopped = {
  active: false, deviceId: null, targetPackage: null, expiresAt: null, message: "stopped",
};

test("automatic expiry clears running capability state and stale target", () => {
  const running = reconcile(ready, started, "device-a");
  assert.equal(running.helperRunning, true);
  const expired = reconcile(running, stopped, "device-a");
  assert.equal(expired.helperRunning, false);
  assert.equal(expired.activeTargetPackage, null);
  assert.equal(expired.expiresAt, null);
  assert.notEqual(expired.message, started.message);
  assert.equal(expired.vpnAuthorized, true);
  assert.equal(running.helperRunning, true); // Inputs remain immutable.
});

test("a completed status overrides a stale running capability refresh", () => {
  const stale = reconcile(ready, started, "device-a");
  assert.equal(reconcile(stale, stopped, "device-a").helperRunning, false);
});

test("events for another device do not mark the selected device as running", () => {
  const state = reconcile(ready, started, "device-b");
  assert.equal(state.helperRunning, false);
  assert.equal(state.activeTargetPackage, null);
});

test("status events preserve setup and authorization failures", () => {
  const unauthorized = { ...ready, supported: false, vpnAuthorized: false, message: "authorize" };
  assert.equal(reconcile(unauthorized, stopped, "device-a").message, "authorize");
  assert.equal(reconcile(null, stopped, "device-a"), null);
});
