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

  // Buzzer audio (Web Audio API) — same instance-identity discipline as
  // runRowRefs above: an oscillator is created once per powered-on stretch
  // and never recreated on every tick, only started/stopped on an actual
  // false->true / true->false transition (see syncBuzzers). audioCtx is
  // created lazily inside enterRunMode(), which only ever runs from a real
  // click — a genuine user gesture, satisfying browser autoplay policy.
  let audioCtx = null;
  const buzzerOscillators = {}; // inst -> { osc, gain }

  function ensureAudioCtx() {
    if (audioCtx) return audioCtx;
    const Ctor = window.AudioContext || window.webkitAudioContext;
    if (!Ctor) return null; // no Web Audio support (or a headless test env) — silently skip
    audioCtx = new Ctor();
    return audioCtx;
  }

  function startBuzzer(inst) {
    if (buzzerOscillators[inst]) return;
    const ctx = ensureAudioCtx();
    if (!ctx) return;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = "square";
    osc.frequency.value = 2700; // a plain "robot is alive" piezo-style tone
    const now = ctx.currentTime;
    gain.gain.setValueAtTime(0, now);
    gain.gain.linearRampToValueAtTime(0.15, now + 0.01); // short ramp, no click
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    buzzerOscillators[inst] = { osc, gain };
  }

  function stopBuzzer(inst) {
    const b = buzzerOscillators[inst];
    if (!b) return;
    delete buzzerOscillators[inst];
    const now = audioCtx ? audioCtx.currentTime : 0;
    try {
      b.gain.gain.setValueAtTime(b.gain.gain.value, now);
      b.gain.gain.linearRampToValueAtTime(0, now + 0.03);
      b.osc.stop(now + 0.04);
    } catch (e) {
      // Already stopped/disconnected — nothing to clean up.
    }
  }

  function stopAllBuzzers() {
    for (const inst of Object.keys(buzzerOscillators)) stopBuzzer(inst);
  }

  function syncBuzzers(prevState) {
    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      if (!part || part.kind !== "buzzer") continue;
      const wasPowered = !!(prevState.instances || {})[inst]?.powered;
      const isPowered = !!(runState.instances || {})[inst]?.powered;
      if (!wasPowered && isPowered) startBuzzer(inst);
      else if (wasPowered && !isPowered) stopBuzzer(inst);
    }
  }

  function ledGlowColor(inst) {
    const partId = nl.instances[inst] || "";
    return partId.includes("green") ? "#57b48f" : partId.includes("red") ? "#e05c50" : "#e8a33d";
  }

  // An unprotected, powered LED isn't "extra bright" — E33's own words are
  // "would burn out instantly" (checks.rs). Show the actual consequence of
  // running this circuit: a scorched, cracked bulb with smoke, not a
  // brighter version of "working". Instantaneous, not animated — this is a
  // pure function of the current input like everything else here, not a
  // timed sequence (no persistent damage tracking across edits either: fix
  // the circuit and it's a normal LED again, matching the no-timestep
  // model the rest of run mode already commits to).
  function drawBurnedLed(x, y, r) {
    cx.save();
    cx.fillStyle = "#1a1512";
    cx.beginPath(); cx.arc(x, y, r, 0, Math.PI * 2); cx.fill();
    cx.strokeStyle = "#3a2f28"; cx.lineWidth = 1.5;
    cx.beginPath(); cx.arc(x, y, r, 0, Math.PI * 2); cx.stroke();
    cx.strokeStyle = "#0a0807"; cx.lineWidth = 1;
    cx.beginPath();
    cx.moveTo(x - r * 0.5, y - r * 0.4);
    cx.lineTo(x - r * 0.1, y);
    cx.lineTo(x + r * 0.3, y - r * 0.2);
    cx.lineTo(x + r * 0.5, y + r * 0.5);
    cx.stroke();
    cx.strokeStyle = "#8b969b"; cx.lineWidth = 1.5; cx.globalAlpha = 0.5;
    for (const dx of [-r * 0.4, r * 0.4]) {
      cx.beginPath();
      cx.moveTo(x + dx, y - r);
      cx.quadraticCurveTo(x + dx - 4, y - r - 8, x + dx + 3, y - r - 16);
      cx.stroke();
    }
    cx.restore();
  }

  function ledGlow(x, y, r, inst, s) {
    const linear = Math.min(1, (s.current_a ?? 0) / 0.02);
    const brightness = Math.pow(linear, 2.2);
    cx.save();
    cx.shadowColor = ledGlowColor(inst);
    cx.shadowBlur = 2 + brightness * 26;
    cx.fillStyle = ledGlowColor(inst);
    cx.globalAlpha = 0.04 + brightness * 0.81;
    cx.beginPath(); cx.arc(x, y, r, 0, Math.PI * 2); cx.fill();
    cx.restore();
  }

  function ledOff(x, y, r, inst) {
    // Clearly OFF, not just "no glow drawn" — a dim, outlined bulb in the
    // LED's own color reads unambiguously as "off", where absence of any
    // marking could just as easily read as "not rendered yet".
    cx.save();
    cx.fillStyle = ledGlowColor(inst);
    cx.globalAlpha = 0.1;
    cx.beginPath(); cx.arc(x, y, r, 0, Math.PI * 2); cx.fill();
    cx.globalAlpha = 1;
    cx.strokeStyle = "#565f66"; cx.lineWidth = 1.5;
    cx.beginPath(); cx.arc(x, y, r, 0, Math.PI * 2); cx.stroke();
    cx.restore();
  }

  function closedOutline2d(g, s) {
    if (!s.closed) return;
    cx.save();
    cx.strokeStyle = "#57b48f"; cx.lineWidth = 3;
    roundRect(g.x - g.w / 2, g.y - g.h / 2, g.w, g.h, 6);
    cx.stroke();
    cx.restore();
  }

  function closedDot3d(q, s) {
    if (!s.closed) return;
    cx.fillStyle = "#57b48f";
    cx.beginPath(); cx.arc(q[0], q[1], 5, 0, Math.PI * 2); cx.fill();
  }

  // --- Per-kind run-mode component registry ------------------------------
  // Each kind's complete run-mode behavior — default input, click handling,
  // panel row markup + wiring, live readout text, and canvas overlay — in
  // one place, dispatched generically by the call sites below rather than
  // scattered across all of them (the gap that let the motor's own
  // "powered" state go unrendered for a while: nothing here forced every
  // kind's behavior to live somewhere findable). Kinds absent here
  // (regulator/esc/radio/buzzer/servo) fall through to each call site's
  // generic default (a plain readout span showing "powered/unpowered ·
  // X.XXA"; no overlay) — genuinely identical behavior across those five,
  // not worth a registry entry each. `mcu` gets its own entry below (its
  // drivable pins are the run-mode signal source for a motor's throttle),
  // but still falls through to that same generic readout for its own
  // powered/current status, having no `updateReadout` of its own.
  const RUN_COMPONENTS = {
    switch: {
      defaultInput: (inst, part, inputs) => { inputs.switches[inst] = false; },
      handlePointerDown: (inst) => { toggleSwitch(inst); return true; },
      panelControl: () => `<button class="mini runToggle" data-kind="switch"></button>`,
      wireRow: (inst, refs) => {
        if (refs.toggle) refs.toggle.addEventListener("click", () => toggleSwitch(inst));
      },
      drawOverlay2d: (inst, g, s) => closedOutline2d(g, s),
      drawOverlay3d: (inst, q, s) => closedDot3d(q, s),
    },
    button: {
      defaultInput: (inst, part, inputs) => { inputs.buttons[inst] = false; },
      handlePointerDown: (inst) => { heldButtonInst = inst; setButtonHeld(inst, true); return true; },
      panelControl: () => `<button class="mini runToggle" data-kind="button"></button>`,
      wireRow: (inst, refs) => {
        if (refs.toggle) refs.toggle.addEventListener("pointerdown", () => { heldButtonInst = inst; setButtonHeld(inst, true); });
      },
      drawOverlay2d: (inst, g, s) => closedOutline2d(g, s),
      drawOverlay3d: (inst, q, s) => closedDot3d(q, s),
    },
    motor: {
      // No slider of its own: a motor's throttle is commanded by whichever
      // MCU pin its driver channel actually resolves to (see the `mcu`
      // entry below and `robowire::signal`) — pinning a control directly to
      // the motor instance would let it spin with no signal wiring at all,
      // exactly the shortcut that made run mode unable to catch a real
      // brain-to-ESC wiring mistake.
      updateReadout: (inst, part, s, refs) => {
        refs.readout.textContent =
          `spin ${(s.spin ?? 0).toFixed(2)}` + (s.current_a != null ? ` · ${s.current_a.toFixed(2)}A` : "");
      },
      drawOverlay2d: (inst, g, s) => {
        // The driver channel being powered is a distinct, always-visible
        // state from actually spinning — a motor idling at zero throttle on
        // a live rail otherwise looks identical to one with no power
        // reaching it at all (the spin tick only appears once throttle !=
        // 0), the same gap switch/LED already close with a visible
        // closed/lit state independent of anything else moving.
        if (s.powered) {
          cx.save();
          cx.strokeStyle = "#57b48f"; cx.lineWidth = 2;
          roundRect(g.x - g.w / 2, g.y - g.h / 2, g.w, g.h, 6);
          cx.stroke();
          cx.restore();
        }
        if (Math.abs(s.spin || 0) > 0.001) {
          cx.save();
          cx.translate(g.x, g.y);
          cx.rotate(spinPhase * Math.sign(s.spin));
          cx.strokeStyle = "#e8a33d"; cx.lineWidth = 2;
          cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, -Math.min(g.w, g.h) / 2 - 2); cx.stroke();
          cx.restore();
        }
        if (s.current_a != null) {
          cx.fillStyle = "#c7ced2";
          cx.font = "9px ui-monospace, Menlo, monospace";
          cx.fillText(`${s.current_a.toFixed(2)}A`, g.x - g.w / 2 + 8, g.y + g.h / 2 - 6);
        }
      },
      drawOverlay3d: (inst, q, s) => {
        // Same "powered, independent of spin" distinction as the 2D overlay.
        if (s.powered) {
          cx.fillStyle = "#57b48f";
          cx.beginPath(); cx.arc(q[0] - 10, q[1] - 10, 3, 0, Math.PI * 2); cx.fill();
        }
        if (Math.abs(s.spin || 0) > 0.001) {
          cx.save();
          cx.translate(q[0], q[1]);
          cx.rotate(spinPhase * Math.sign(s.spin));
          cx.strokeStyle = "#e8a33d"; cx.lineWidth = 2;
          cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, -10); cx.stroke();
          cx.restore();
        }
      },
    },
    potentiometer: {
      defaultInput: (inst, part, inputs) => { inputs.dial_positions[inst] = 0.5; },
      panelControl: (inst) =>
        `<input type="range" class="runDial" min="0" max="1" step="0.01" value="${runInputs.dial_positions[inst] ?? 0.5}"> <span class="runReadout"></span>`,
      wireRow: (inst, refs) => {
        const dial = refs.row.querySelector(".runDial");
        if (dial) dial.addEventListener("input", () => setDialPosition(inst, parseFloat(dial.value)));
      },
      updateReadout: (inst, part, s, refs) => {
        refs.readout.textContent = `${Math.round((runInputs.dial_positions[inst] ?? 0.5) * 100)}%`;
      },
    },
    tof: {
      defaultInput: (inst, part, inputs) => { inputs.sensor_values[inst] = part.range_mm ?? 0; },
      panelControl: (inst) =>
        `<input type="number" class="runSensor" value="${runInputs.sensor_values[inst] ?? 0}"> <span class="runReadout"></span>`,
      wireRow: (inst, refs) => {
        const sensor = refs.row.querySelector(".runSensor");
        if (sensor) sensor.addEventListener("change", () => setSensorValue(inst, parseFloat(sensor.value) || 0));
      },
      updateReadout: (inst, part, s, refs) => {
        refs.readout.innerHTML =
          `${s.powered ? "live" : "unpowered"}` +
          (s.bus_conflict ? ` <span style="color:var(--bad);font-weight:700">ADDRESS CONFLICT</span>` : "");
      },
      drawOverlay2d: (inst, g, s) => {
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
      },
      drawOverlay3d: (inst, q, s) => {
        cx.fillStyle = s.bus_conflict ? "#e05c50" : s.powered ? "#57b48f" : "#4a575f";
        cx.beginPath(); cx.arc(q[0] + 10, q[1] - 10, 3, 0, Math.PI * 2); cx.fill();
      },
    },
    mcu: {
      // One slider per pin the sim itself reports as actually driving
      // something (`s.pwm_channels`, from `robowire::signal::mcu_drivable_pins`)
      // — this UI never guesses which pins are "drivable" on its own; it
      // only ever renders what robosim already resolved from the real
      // wiring. Standing in for a signal generator/RC receiver you'd hook
      // up on a bench, one pin at a time, before any firmware exists.
      panelControl: (inst) => {
        const channels = (runState.instances[inst] || {}).pwm_channels || [];
        if (!channels.length) return `<span class="runReadout"></span>`;
        const caption = `<div class="d" style="color:var(--dim);font-size:10px">` +
          `standing in for a signal generator / RC receiver — no firmware runs here</div>`;
        const rows = channels.map(c => {
          const pin = `${inst}.${c.pin}`;
          const label = c.drives ? `${c.pin} → ${c.drives}` : `${c.pin} (unconnected downstream)`;
          const v = runInputs.pwm_signals[pin] ?? 0;
          return `<div class="d" style="margin-top:4px">${label}` +
            `<input type="range" class="runPwm" data-pin="${pin}" min="-1" max="1" step="0.05" value="${v}">` +
            `</div>`;
        });
        return caption + rows.join("") + `<span class="runReadout"></span>`;
      },
      wireRow: (inst, refs) => {
        for (const el of refs.row.querySelectorAll(".runPwm")) {
          const pin = el.dataset.pin;
          el.addEventListener("input", () => setPwmSignal(pin, parseFloat(el.value)));
        }
      },
    },
    led: {
      // Brightness tracks live current (20mA ~ a typical indicator LED's
      // rated forward current, used only as a "what counts as fully
      // bright" reference for this glow — not a declared/authoritative
      // figure). Gamma-corrected (^2.2, the standard display gamma): human
      // brightness perception is far more sensitive at low light levels
      // than current itself is linear, so a LINEAR current->alpha mapping
      // still looks "clearly on" well below rated current. No alpha/blur
      // floor either, so it can fade all the way toward the off-state
      // look. `burned` (no series resistor) takes priority over both.
      drawOverlay2d: (inst, g, s) => {
        const r = Math.min(g.w, g.h) / 2 + 4;
        if (s.burned) drawBurnedLed(g.x, g.y, r);
        else if (s.lit) ledGlow(g.x, g.y, r, inst, s);
        else ledOff(g.x, g.y, r, inst);
      },
      drawOverlay3d: (inst, q, s) => {
        if (s.burned) drawBurnedLed(q[0], q[1], 8);
        else if (s.lit) ledGlow(q[0], q[1], 8, inst, s);
        else ledOff(q[0], q[1], 8, inst);
      },
    },
  };
  // imu shares every tof behavior except its default fake reading (0, not
  // range_mm — imu has no "range").
  RUN_COMPONENTS.imu = { ...RUN_COMPONENTS.tof, defaultInput: (inst, part, inputs) => { inputs.sensor_values[inst] = 0; } };

  function defaultRunInputs() {
    const inputs = { switches: {}, buttons: {}, pwm_signals: {}, dial_positions: {}, sensor_values: {} };
    for (const [inst, partId] of Object.entries(nl.instances)) {
      const part = partById[partId];
      if (!part) continue;
      RUN_COMPONENTS[part.kind]?.defaultInput?.(inst, part, inputs);
    }
    return inputs;
  }

  function updateRunState() {
    if (!runMode) return;
    const prev = runState;
    runState = callRunState(nl, runInputs);
    syncBuzzers(prev);
    renderRunPanel();
    draw();
  }

  function toggleSwitch(inst) { runInputs.switches[inst] = !runInputs.switches[inst]; updateRunState(); }
  function setDialPosition(inst, v) { runInputs.dial_positions[inst] = v; updateRunState(); }
  function setButtonHeld(inst, held) { runInputs.buttons[inst] = held; updateRunState(); }
  function setPwmSignal(pin, v) { runInputs.pwm_signals[pin] = v; updateRunState(); }
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
    return RUN_COMPONENTS[kind]?.handlePointerDown?.(inst) ?? false;
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
    if (teachMode) exitTeachMode();
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
    stopAllBuzzers();
    if (audioCtx) {
      if (audioCtx.close) audioCtx.close();
      audioCtx = null;
    }
    const left = document.getElementById("left");
    left.style.pointerEvents = "";
    left.style.opacity = "";
    document.getElementById("arrangeBtn").disabled = false;
    document.getElementById("importBtn").disabled = false;
    const btn = document.getElementById("runBtn");
    btn.textContent = "run mode";
    btn.classList.remove("primary");
    const panel = document.getElementById("runPanel");
    panel.style.display = "none";
    panel.innerHTML = "";
    runRowRefs = {}; // rows are keyed by instance name and built once per
    // entry (see buildRunPanelRow) — without this, re-entering run mode
    // after loading a different netlist would keep showing stale rows for
    // instances that no longer exist (or, worse, reuse a row built for a
    // different kind under the same instance name).
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
    const comp = RUN_COMPONENTS[part.kind];
    let body = `<div class="hd"><b>${inst}</b> <span class="k">${part.kind}</span></div>`;
    body += comp?.panelControl ? comp.panelControl(inst, part) : `<span class="runReadout"></span>`;
    body += `<div class="d runReason" style="margin-top:3px;color:var(--dim)"></div>`;
    row.innerHTML = body;

    const refs = { row, readout: row.querySelector(".runReadout"), reason: row.querySelector(".runReason") };
    const toggle = row.querySelector(".runToggle");
    if (toggle) refs.toggle = toggle;
    comp?.wireRow?.(inst, refs);
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
        const comp = RUN_COMPONENTS[part.kind];
        if (comp?.updateReadout) {
          comp.updateReadout(inst, part, s, refs);
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

  function drawRunOverlay2d(inst, g) {
    const s = (runState.instances || {})[inst];
    if (!s) return;
    RUN_COMPONENTS[kindOf(inst)]?.drawOverlay2d?.(inst, g, s);
  }

  function drawRunOverlay3d(inst, q) {
    const s = (runState.instances || {})[inst];
    if (!s) return;
    RUN_COMPONENTS[kindOf(inst)]?.drawOverlay3d?.(inst, q, s);
  }

  document.getElementById("runBtn").addEventListener("click", () => { runMode ? exitRunMode() : enterRunMode(); });
