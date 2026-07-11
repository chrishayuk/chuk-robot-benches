// designer/12-run.js — interactive run mode (specs/robowire.md §3a): click a
// switch, the LED lights. Event-driven, no firmware, no timeline — every
// part's rendered state is a pure function of the current switch/button/
// throttle/sensor inputs, recomputed via the real wasm run_state engine on
// every change (same code as the CLI/tests, per design-servers discipline).
// State (runMode, runState, runInputs, ...) lives in 01-state.js — see the
// comment there for why.

  function defaultRunInputs() {
    const inputs = { switches: {}, buttons: {}, throttles: {}, sensor_values: {} };
    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      if (!part) continue;
      if (part.kind === "switch") inputs.switches[inst] = false;
      if (part.kind === "button") inputs.buttons[inst] = false;
      if (part.kind === "motor") inputs.throttles[inst] = 0;
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

  function renderRunPanel() {
    const el = document.getElementById("runPanel");
    if (!runMode) { el.innerHTML = ""; return; }
    el.innerHTML = "";
    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      if (!part) continue;
      const s = (runState.instances || {})[inst] || {};
      let body = `<div class="hd"><b>${inst}</b> <span class="k">${part.kind}</span></div>`;
      if (part.kind === "switch" || part.kind === "button") {
        body += `<button class="mini runToggle" data-inst="${inst}" data-kind="${part.kind}">${s.closed ? "ON" : "off"}</button>`;
      } else if (part.kind === "motor") {
        body += `<input type="range" class="runThrottle" data-inst="${inst}" min="-1" max="1" step="0.05" value="${runInputs.throttles[inst] ?? 0}"> spin ${(s.spin ?? 0).toFixed(2)}` +
          (s.current_a != null ? ` · ${s.current_a.toFixed(2)}A` : "");
      } else if (part.kind === "tof" || part.kind === "imu") {
        body += `<input type="number" class="runSensor" data-inst="${inst}" value="${runInputs.sensor_values[inst] ?? 0}"> ${s.powered ? "live" : "unpowered"}` +
          (s.bus_conflict ? ` <span style="color:var(--bad);font-weight:700">ADDRESS CONFLICT</span>` : "");
      } else if (s.powered !== undefined) {
        body += ` — ${s.powered ? "powered" : "unpowered"}` + (s.current_a != null ? ` · ${s.current_a.toFixed(2)}A` : "");
      }
      if (s.reason) body += `<div class="d" style="margin-top:3px;color:var(--dim)">${s.reason}</div>`;
      const row = document.createElement("div");
      row.className = "row runrow";
      row.innerHTML = body;
      el.appendChild(row);
    }
    el.querySelectorAll(".runToggle").forEach(btn => {
      const inst = btn.dataset.inst;
      if (btn.dataset.kind === "switch") {
        btn.addEventListener("click", () => toggleSwitch(inst));
      } else {
        btn.addEventListener("pointerdown", () => { heldButtonInst = inst; setButtonHeld(inst, true); });
      }
    });
    el.querySelectorAll(".runThrottle").forEach(inp => {
      inp.addEventListener("input", () => setThrottle(inp.dataset.inst, parseFloat(inp.value)));
    });
    el.querySelectorAll(".runSensor").forEach(inp => {
      inp.addEventListener("change", () => setSensorValue(inp.dataset.inst, parseFloat(inp.value) || 0));
    });
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
    if (kind === "led" && s.lit) {
      cx.save();
      cx.shadowColor = ledGlowColor(inst);
      cx.shadowBlur = s.current_limited ? 16 : 28;
      cx.fillStyle = ledGlowColor(inst);
      cx.globalAlpha = 0.85;
      cx.beginPath(); cx.arc(g.x, g.y, Math.min(g.w, g.h) / 2 + 4, 0, Math.PI * 2); cx.fill();
      cx.restore();
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
    if (kind === "led" && s.lit) {
      cx.save();
      cx.shadowColor = ledGlowColor(inst);
      cx.shadowBlur = s.current_limited ? 16 : 28;
      cx.fillStyle = ledGlowColor(inst);
      cx.globalAlpha = 0.85;
      cx.beginPath(); cx.arc(q[0], q[1], 8, 0, Math.PI * 2); cx.fill();
      cx.restore();
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
