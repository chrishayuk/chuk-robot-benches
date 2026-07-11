// designer/12-run.js — interactive run mode (specs/robowire.md §3a): click a
// switch, the LED lights. Event-driven, no firmware, no timeline — every
// part's rendered state is a pure function of the current switch/button/
// throttle/sensor inputs, recomputed via the real wasm run_state engine on
// every change (same code as the CLI/tests, per design-servers discipline).
// State (runMode, runState, runInputs, ...) lives in 01-state.js — see the
// comment there for why.

  // inst -> { row, readout, reason, toggle? } — the run panel's per-instance
  // interactive elements, built once (see renderRunPanel). Only ever read
  // from within run-mode functions (never during the initial synchronous
  // page-load draw()), so no TDZ hazard declaring it here.
  let runRowRefs = {};

  function defaultRunInputs() {
    const inputs = { switches: {}, buttons: {}, throttles: {}, dial_positions: {}, sensor_values: {} };
    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      if (!part) continue;
      if (part.kind === "switch") inputs.switches[inst] = false;
      if (part.kind === "button") inputs.buttons[inst] = false;
      if (part.kind === "motor") inputs.throttles[inst] = 0;
      if (part.kind === "potentiometer") inputs.dial_positions[inst] = 0.5;
      if (part.kind === "tof") inputs.sensor_values[inst] = part.range_mm ?? 0;
      if (part.kind === "imu") inputs.sensor_values[inst] = 0;
    }
    return inputs;
  }

  function updateRunState() {
    if (!runMode) return;
    runState = callRunState(nl, runInputs);
    renderRunPanel();
    draw();
  }

  function toggleSwitch(inst) { runInputs.switches[inst] = !runInputs.switches[inst]; updateRunState(); }
  function setDialPosition(inst, v) { runInputs.dial_positions[inst] = v; updateRunState(); }
  function setButtonHeld(inst, held) { runInputs.buttons[inst] = held; updateRunState(); }
  function setThrottle(inst, v) { runInputs.throttles[inst] = v; updateRunState(); }
  function setSensorValue(inst, v) { runInputs.sensor_values[inst] = v; updateRunState(); }

  // Global release, not just cv's pointerup: a hold started from the side
  // panel (outside the canvas) still needs to let go wherever the pointer
  // comes up.
  window.addEventListener("pointerup", () => {
    if (heldButtonInst) { const inst = heldButtonInst; heldButtonInst = null; setButtonHeld(inst, false); }
  });

  function handleRunPointerDown(mx, my) {
    const inst = instAt(mx, my);
    if (!inst) return false;
    const kind = kindOf(inst);
    if (kind === "switch") { toggleSwitch(inst); return true; }
    if (kind === "button") { heldButtonInst = inst; setButtonHeld(inst, true); return true; }
    return false;
  }

  function ensureSpinLoop() {
    if (spinRAF) return;
    const tick = () => {
      spinRAF = null;
      if (!runMode) return;
      const spinning = Object.values(runState.instances || {}).some(s => Math.abs(s.spin || 0) > 0.001);
      const flowing = Object.values(runState.nets || {}).some(n => n.amps > 0.001);
      spinPhase += 0.12;
      if (spinning || flowing) draw();
      spinRAF = requestAnimationFrame(tick);
    };
    spinRAF = requestAnimationFrame(tick);
  }

  function enterRunMode() {
    runMode = true;
    runInputs = defaultRunInputs();
    const left = document.getElementById("left");
    left.style.pointerEvents = "none";
    left.style.opacity = "0.4";
    document.getElementById("arrangeBtn").disabled = true;
    document.getElementById("importBtn").disabled = true;
    const btn = document.getElementById("runBtn");
    btn.textContent = "exit run mode";
    btn.classList.add("primary");
    document.getElementById("runPanel").style.display = "";
    selInst = null; selNet = -1; pending = null; wireDrag = null;
    updateRunState();
    ensureSpinLoop();
  }
  function exitRunMode() {
    runMode = false;
    heldButtonInst = null;
    const left = document.getElementById("left");
    left.style.pointerEvents = "";
    left.style.opacity = "";
    document.getElementById("arrangeBtn").disabled = false;
    document.getElementById("importBtn").disabled = false;
    const btn = document.getElementById("runBtn");
    btn.textContent = "run mode";
    btn.classList.remove("primary");
    document.getElementById("runPanel").style.display = "none";
    draw();
  }

  // Each instance's row is built ONCE and its <input>/<button> elements are
  // never destroyed afterward — only their surrounding readout text is
  // patched on each tick. Rebuilding the whole panel on every input event
  // (the previous design) ripped the very slider the user was mid-drag on
  // out of the DOM the instant that drag's own `input` event re-rendered
  // it, silently ending the gesture after a pixel of movement.
  function buildRunPanelRow(inst, part) {
    const row = document.createElement("div");
    row.className = "row runrow";
    let body = `<div class="hd"><b>${inst}</b> <span class="k">${part.kind}</span></div>`;
    if (part.kind === "switch" || part.kind === "button") {
      body += `<button class="mini runToggle" data-kind="${part.kind}"></button>`;
    } else if (part.kind === "motor") {
      body += `<input type="range" class="runThrottle" min="-1" max="1" step="0.05" value="${runInputs.throttles[inst] ?? 0}"> <span class="runReadout"></span>`;
    } else if (part.kind === "potentiometer") {
      body += `<input type="range" class="runDial" min="0" max="1" step="0.01" value="${runInputs.dial_positions[inst] ?? 0.5}"> <span class="runReadout"></span>`;
    } else if (part.kind === "tof" || part.kind === "imu") {
      body += `<input type="number" class="runSensor" value="${runInputs.sensor_values[inst] ?? 0}"> <span class="runReadout"></span>`;
    } else {
      body += `<span class="runReadout"></span>`;
    }
    body += `<div class="d runReason" style="margin-top:3px;color:var(--dim)"></div>`;
    row.innerHTML = body;

    const refs = { row, readout: row.querySelector(".runReadout"), reason: row.querySelector(".runReason") };
    const toggle = row.querySelector(".runToggle");
    if (toggle) {
      refs.toggle = toggle;
      if (part.kind === "switch") toggle.addEventListener("click", () => toggleSwitch(inst));
      else toggle.addEventListener("pointerdown", () => { heldButtonInst = inst; setButtonHeld(inst, true); });
    }
    const throttle = row.querySelector(".runThrottle");
    if (throttle) throttle.addEventListener("input", () => setThrottle(inst, parseFloat(throttle.value)));
    const dial = row.querySelector(".runDial");
    if (dial) dial.addEventListener("input", () => setDialPosition(inst, parseFloat(dial.value)));
    const sensor = row.querySelector(".runSensor");
    if (sensor) sensor.addEventListener("change", () => setSensorValue(inst, parseFloat(sensor.value) || 0));
    return refs;
  }

  function renderRunPanel() {
    const el = document.getElementById("runPanel");
    if (!runMode) { el.innerHTML = ""; runRowRefs = {}; return; }

    for (const [inst, partId] of Object.entries(nl.instances)) {
      if (runRowRefs[inst]) continue;
      const part = partById[partId];
      if (!part) continue;
      runRowRefs[inst] = buildRunPanelRow(inst, part);
      el.appendChild(runRowRefs[inst].row);
    }

    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      const refs = runRowRefs[inst];
      if (!part || !refs) continue;
      const s = (runState.instances || {})[inst] || {};
      if (refs.toggle) refs.toggle.textContent = s.closed ? "ON" : "off";
      if (refs.readout) {
        if (part.kind === "motor") {
          refs.readout.textContent = `spin ${(s.spin ?? 0).toFixed(2)}` + (s.current_a != null ? ` · ${s.current_a.toFixed(2)}A` : "");
        } else if (part.kind === "potentiometer") {
          refs.readout.textContent = `${Math.round((runInputs.dial_positions[inst] ?? 0.5) * 100)}%`;
        } else if (part.kind === "tof" || part.kind === "imu") {
          refs.readout.innerHTML = `${s.powered ? "live" : "unpowered"}` +
            (s.bus_conflict ? ` <span style="color:var(--bad);font-weight:700">ADDRESS CONFLICT</span>` : "");
        } else if (s.powered !== undefined) {
          refs.readout.textContent = `${s.powered ? "powered" : "unpowered"}` + (s.current_a != null ? ` · ${s.current_a.toFixed(2)}A` : "");
        }
      }
      if (refs.reason) refs.reason.textContent = s.reason || "";
    }
  }

  // Fallback label anchor for 2-pin nets (netSegments' `hub` is only set for
  // 3+ pin nets) — centroid of the net's pin positions, in whichever view
  // (2D/3D) is active.
  function netLabelPos(net) {
    const pts = net.pins.map(pinXY).filter(Boolean);
    if (!pts.length) return null;
    const x = pts.reduce((s, p) => s + p[0], 0) / pts.length;
    const y = pts.reduce((s, p) => s + p[1], 0) / pts.length;
    return [x, y];
  }

  function ledGlowColor(inst) {
    const partId = nl.instances[inst] || "";
    return partId.includes("green") ? "#57b48f" : partId.includes("red") ? "#e05c50" : "#e8a33d";
  }

  function drawRunOverlay2d(inst, g) {
    const s = (runState.instances || {})[inst];
    if (!s) return;
    const kind = kindOf(inst);
    if (kind === "led") {
      const r = Math.min(g.w, g.h) / 2 + 4;
      if (s.lit) {
        // Brightness tracks live current (20mA ~ a typical indicator LED's
        // rated forward current, used only as a "what counts as fully
        // bright" reference for this glow — not a declared/authoritative
        // figure). Gamma-corrected (^2.2, the standard display gamma): human
        // brightness perception is far more sensitive at low light levels
        // than current itself is linear, so a LINEAR current->alpha mapping
        // still looks "clearly on" well below rated current — a dimmer
        // pushed most of the way down needs to look convincingly dim, not
        // just "a bit less bright". No alpha/blur floor either, so it can
        // fade all the way toward the off-state look.
        const linear = s.current_limited ? Math.min(1, (s.current_a ?? 0) / 0.02) : 1;
        const brightness = s.current_limited ? Math.pow(linear, 2.2) : 1;
        cx.save();
        cx.shadowColor = ledGlowColor(inst);
        cx.shadowBlur = s.current_limited ? 2 + brightness * 26 : 28;
        cx.fillStyle = ledGlowColor(inst);
        cx.globalAlpha = s.current_limited ? 0.04 + brightness * 0.81 : 0.85;
        cx.beginPath(); cx.arc(g.x, g.y, r, 0, Math.PI * 2); cx.fill();
        cx.restore();
      } else {
        // Clearly OFF, not just "no glow drawn" — a dim, outlined bulb in
        // the LED's own color reads unambiguously as "off", where absence
        // of any marking could just as easily read as "not rendered yet".
        cx.save();
        cx.fillStyle = ledGlowColor(inst);
        cx.globalAlpha = 0.1;
        cx.beginPath(); cx.arc(g.x, g.y, r, 0, Math.PI * 2); cx.fill();
        cx.globalAlpha = 1;
        cx.strokeStyle = "#565f66"; cx.lineWidth = 1.5;
        cx.beginPath(); cx.arc(g.x, g.y, r, 0, Math.PI * 2); cx.stroke();
        cx.restore();
      }
    }
    if ((kind === "switch" || kind === "button") && s.closed) {
      cx.save();
      cx.strokeStyle = "#57b48f"; cx.lineWidth = 3;
      roundRect(g.x - g.w / 2, g.y - g.h / 2, g.w, g.h, 6);
      cx.stroke();
      cx.restore();
    }
    if (kind === "motor" && Math.abs(s.spin || 0) > 0.001) {
      cx.save();
      cx.translate(g.x, g.y);
      cx.rotate(spinPhase * Math.sign(s.spin));
      cx.strokeStyle = "#e8a33d"; cx.lineWidth = 2;
      cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, -Math.min(g.w, g.h) / 2 - 2); cx.stroke();
      cx.restore();
    }
    if (kind === "motor" && s.current_a != null) {
      cx.fillStyle = "#c7ced2";
      cx.font = "9px ui-monospace, Menlo, monospace";
      cx.fillText(`${s.current_a.toFixed(2)}A`, g.x - g.w / 2 + 8, g.y + g.h / 2 - 6);
    }
    if (kind === "tof" || kind === "imu") {
      cx.save();
      cx.fillStyle = s.powered ? "#57b48f" : "#4a575f";
      cx.beginPath(); cx.arc(g.x + g.w / 2 - 8, g.y + g.h / 2 - 8, 3, 0, Math.PI * 2); cx.fill();
      if (s.value !== undefined) {
        cx.fillStyle = "#c7ced2";
        cx.font = "9px ui-monospace, Menlo, monospace";
        cx.fillText(Number(s.value).toFixed(0), g.x - g.w / 2 + 8, g.y + g.h / 2 - 6);
      }
      if (s.bus_conflict) {
        cx.fillStyle = "#e05c50";
        cx.font = "bold 9px ui-monospace, Menlo, monospace";
        cx.fillText("CONFLICT", g.x - g.w / 2 + 8, g.y + g.h / 2 + 8);
      }
      cx.restore();
    }
  }

  function drawRunOverlay3d(inst, q) {
    const s = (runState.instances || {})[inst];
    if (!s) return;
    const kind = kindOf(inst);
    if (kind === "led") {
      if (s.lit) {
        // Brightness tracks live current (20mA ~ a typical indicator LED's
        // rated forward current, used only as a "what counts as fully
        // bright" reference for this glow — not a declared/authoritative
        // figure). Gamma-corrected (^2.2, the standard display gamma): human
        // brightness perception is far more sensitive at low light levels
        // than current itself is linear, so a LINEAR current->alpha mapping
        // still looks "clearly on" well below rated current — a dimmer
        // pushed most of the way down needs to look convincingly dim, not
        // just "a bit less bright". No alpha/blur floor either, so it can
        // fade all the way toward the off-state look.
        const linear = s.current_limited ? Math.min(1, (s.current_a ?? 0) / 0.02) : 1;
        const brightness = s.current_limited ? Math.pow(linear, 2.2) : 1;
        cx.save();
        cx.shadowColor = ledGlowColor(inst);
        cx.shadowBlur = s.current_limited ? 2 + brightness * 26 : 28;
        cx.fillStyle = ledGlowColor(inst);
        cx.globalAlpha = s.current_limited ? 0.04 + brightness * 0.81 : 0.85;
        cx.beginPath(); cx.arc(q[0], q[1], 8, 0, Math.PI * 2); cx.fill();
        cx.restore();
      } else {
        // Clearly OFF, not just "no glow drawn" — see the 2D overlay for why.
        cx.save();
        cx.fillStyle = ledGlowColor(inst);
        cx.globalAlpha = 0.1;
        cx.beginPath(); cx.arc(q[0], q[1], 8, 0, Math.PI * 2); cx.fill();
        cx.globalAlpha = 1;
        cx.strokeStyle = "#565f66"; cx.lineWidth = 1.5;
        cx.beginPath(); cx.arc(q[0], q[1], 8, 0, Math.PI * 2); cx.stroke();
        cx.restore();
      }
    }
    if ((kind === "switch" || kind === "button") && s.closed) {
      cx.fillStyle = "#57b48f";
      cx.beginPath(); cx.arc(q[0], q[1], 5, 0, Math.PI * 2); cx.fill();
    }
    if (kind === "motor" && Math.abs(s.spin || 0) > 0.001) {
      cx.save();
      cx.translate(q[0], q[1]);
      cx.rotate(spinPhase * Math.sign(s.spin));
      cx.strokeStyle = "#e8a33d"; cx.lineWidth = 2;
      cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, -10); cx.stroke();
      cx.restore();
    }
    if (kind === "tof" || kind === "imu") {
      cx.fillStyle = s.bus_conflict ? "#e05c50" : s.powered ? "#57b48f" : "#4a575f";
      cx.beginPath(); cx.arc(q[0] + 10, q[1] - 10, 3, 0, Math.PI * 2); cx.fill();
    }
  }

  document.getElementById("runBtn").addEventListener("click", () => { runMode ? exitRunMode() : enterRunMode(); });
