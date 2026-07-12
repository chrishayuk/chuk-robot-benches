#!/usr/bin/env node
// Standing verification for the assembled designer artifact. Run after any
// regen: node tools/verify-designer.mjs derived/robowire-designer.html
// Checks the file users actually open — not the templates.
import { readFileSync } from "fs";

const path = process.argv[2] || "derived/robowire-designer.html";
const html = readFileSync(path, "utf8");
const fail = msg => { console.error("VERIFY FAIL:", msg); process.exit(1); };

const m = html.match(/<script>([\s\S]*)<\/script>/);
if (!m) fail("no script block");
try { new Function(m[1]); } catch (e) { fail("script does not parse: " + e.message); }

const count = k => (html.match(new RegExp(k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g")) || []).length;
const exactlyOne = [
  "function draw() {", "function draw3()", "function syncCanvas()", "function renderExamples()", "let wireDrag = null",
  "function updateRunState()", "function renderRunPanel()",
  "function syncBuzzers(", "function startBuzzer(", "function stopBuzzer(",
  "function enterTeachMode()", "function exitTeachMode()", "function renderTeachPanel()",
  "function renderTeachLessons()", "function parseLessonName(", "function drawBurnedLed(",
  "function setPwmSignal(",
];
for (const k of exactlyOne) {
  if (count(k) !== 1) fail(`expected exactly one '${k}', found ${count(k)}`);
}
for (const k of ["__BUILD__", "//__PARTS__", "//__NETLIST__", "//__WASM__", "//__EXAMPLES__", "//__LESSONS__", "//__MODULES__"]) {
  if (html.includes(k)) fail(`unreplaced placeholder ${k}`);
}
const build = html.match(/build ([0-9a-f]{8})/);
if (!build) fail("no build stamp");
if (!html.includes("BROKEN ON PURPOSE")) fail("lesson quarantine missing");
console.log(`verify OK: ${path} build ${build[1]}`);
