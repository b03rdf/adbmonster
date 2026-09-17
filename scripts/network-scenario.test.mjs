import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../src/lib/networkScenario.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } });
const {
  defaultScenario, cloneScenario, phaseFromConfig, normalProfile, validateScenario,
  scenarioSeconds, payloadKbps, configDropPercent, formatMeasured,
} = await import("data:text/javascript;base64," + Buffer.from(outputText).toString("base64"));

test("four-stage template is valid, independent and includes outage then recovery", () => {
  const scenario = defaultScenario("com.example.game");
  assert.equal(validateScenario(scenario), null);
  assert.equal(scenarioSeconds(scenario), 40);
  assert.deepEqual(scenario.phases.map(p => p.offline), [false, false, true, false]);
  scenario.phases[0].profile.uploadKbps = 123;
  assert.equal(scenario.phases[3].profile.uploadKbps, 0);
  assert.equal(defaultScenario("com.example.game").phases[0].profile.uploadKbps, 0);
});

test("replay clones both stages and profiles without mutating the report snapshot", () => {
  const original = defaultScenario("com.example.game");
  const replay = cloneScenario(JSON.parse(JSON.stringify(original)));
  assert.deepEqual(replay, original);
  replay.phases[1].profile.latencyMs = 200;
  replay.phases.splice(0, 1);
  replay.seed = 29;
  assert.equal(original.phases[1].profile.latencyMs, 500);
  assert.equal(original.phases.length, 4);
  assert.equal(original.seed, 1);
});

test("single-stage capture excludes device/app fields from the shaping profile", () => {
  const config = { targetPackage: "com.example.game", durationSeconds: 300, ...normalProfile() };
  const phase = phaseFromConfig(config);
  assert.equal(phase.durationSeconds, 300);
  assert.equal("targetPackage" in phase.profile, false);
  assert.equal("durationSeconds" in phase.profile, false);
  phase.profile.uploadKbps = 99;
  assert.equal(config.uploadKbps, 0);
});

test("validation rejects invalid duration, fractional integers, package injection and percent values", () => {
  const mutations = [
    s => { s.targetPackage = "com.a;id"; },
    s => { s.targetPackage = "x".repeat(256) + ".app"; },
    s => { s.seed = -1; },
    s => { s.seed = 4294967296; },
    s => { s.phases = []; },
    s => { s.phases = Array(21).fill(s.phases[0]); },
    s => { s.phases[0].durationSeconds = 0; },
    s => { s.phases[0].durationSeconds = 1.5; },
    s => { s.phases[0].durationSeconds = 3600; },
    s => { s.phases[0].profile.uploadKbps = 1.5; },
    s => { s.phases[0].profile.lossPercent = NaN; },
    s => { s.phases[0].profile.lossPercent = 101; },
    s => { s.phases[0].profile.latencyMs = 0.5; },
    s => { s.phases[0].profile.jitterMs = 1; },
  ];
  for (const mutate of mutations) {
    const scenario = defaultScenario("com.example.game");
    mutate(scenario);
    assert.equal(typeof validateScenario(scenario), "string");
  }
  const boundary = defaultScenario("com.example.game");
  boundary.seed = 4294967295;
  assert.equal(validateScenario(boundary), null);
});

test("throughput counts successful payload bytes over actual elapsed time", () => {
  const counters = { tcpForwardedBytes: 4000, udpForwardedBytes: 4000, tcpReceivedBytes: 16000 };
  assert.equal(payloadKbps(counters, 2000), 32);
  assert.equal(payloadKbps(counters, 0), null);
  assert.equal(formatMeasured(payloadKbps(counters, 0)), "无样本");
});

test("configured loss uses policy trials, excluding overflow and offline drops", () => {
  const counters = { udpPolicyEvaluated: 100, udpConfigDrops: 10, udpQueueOverflowDrops: 20, udpOutageDrops: 30 };
  assert.equal(configDropPercent(counters), 10);
  assert.equal(configDropPercent({ ...counters, udpPolicyEvaluated: 0 }), null);
  assert.equal(formatMeasured(null, "%"), "无样本");
  assert.equal(formatMeasured(0, "%"), "0.00%");
});

