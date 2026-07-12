// designer/13-teach.js — teaching mode (specs/robowire.md §3): a numbered,
// accumulating curriculum (harness/lessons/NN-slug[.json]/NN-slug-broken)
// — "start from the real basics and work up", not a flat pile of one-off
// repair drills (those still exist separately in harness/examples/
// lesson-*.json for the normal sidebar). Backed by the same explain-error
// content `robowire explain-error <CODE>` prints natively (robowire::teach,
// via WASM's explain_error_json — never a second copy of this prose in
// JS). Editing stays LIVE in teach mode — unlike run mode, which locks
// editing because a live circuit isn't meant to be edited while running, a
// repair exercise is precisely about editing the broken netlist until it
// passes.

  function parseLessonName(name) {
    const m = /^(\d+)-(.+?)(-broken)?$/.exec(name || "");
    if (!m) return null;
    return { stage: parseInt(m[1], 10), slug: m[2], broken: !!m[3] };
  }

  function enterTeachMode() {
    if (runMode) exitRunMode();
    teachMode = true;
    teachFocusCode = null;
    const btn = document.getElementById("teachBtn");
    btn.textContent = "exit teaching mode";
    btn.classList.add("primary");
    document.getElementById("palette").style.display = "none";
    document.getElementById("partFilter").style.display = "none";
    refresh();
  }

  function exitTeachMode() {
    teachMode = false;
    teachFocusCode = null;
    const btn = document.getElementById("teachBtn");
    btn.textContent = "teaching mode";
    btn.classList.remove("primary");
    document.getElementById("palette").style.display = "";
    document.getElementById("partFilter").style.display = "";
    refresh();
  }

  function loadLesson(ex) {
    if (Object.keys(nl.instances).length && !confirm("Replace the current design with '" + ex.name + "'?")) return;
    nl = JSON.parse(JSON.stringify(ex));
    if (!nl.failsafe) nl.failsafe = { rx_loss: "", stop_pins: [] };
    if (!nl.buses) nl.buses = [];
    layout = {};
    selInst = null; selNet = -1; pending = null; teachFocusCode = null;
    refresh();
    autoArrange();
  }

  function renderTeachLessons() {
    const el = document.getElementById("examples");
    el.innerHTML = "";
    const hd = document.createElement("div");
    hd.style.cssText = "color:var(--dim);font-size:11px;line-height:1.5;margin-bottom:8px";
    hd.textContent = "Work through these in order — each stage builds on the last. Predict whether it'll pass before you load it, then check the verdict below (click any check row to read why).";
    el.appendChild(hd);

    const stages = new Map(); // stage number -> { legal, broken }
    for (const ex of LESSONS) {
      const p = parseLessonName(ex.name);
      if (!p) continue;
      if (!stages.has(p.stage)) stages.set(p.stage, {});
      stages.get(p.stage)[p.broken ? "broken" : "legal"] = ex;
    }
    const stageNums = [...stages.keys()].sort((a, b) => a - b);
    for (const n of stageNums) {
      const { legal, broken } = stages.get(n);
      const base = legal || broken;
      const title = base.name.replace(/^\d+-/, "").replace(/-broken$/, "").replace(/-/g, " ");
      const loaded = legal && nl.name === legal.name ? " · loaded" : broken && nl.name === broken.name ? " · loaded (broken)" : "";
      const div = document.createElement("div");
      div.className = "part";
      div.innerHTML = `<span><b style="color:var(--accent)">stage ${n}</b> — ${title}${loaded}</span>`;
      if (legal) {
        const btn = document.createElement("button");
        btn.className = "mini";
        btn.textContent = "load";
        btn.addEventListener("click", () => loadLesson(legal));
        div.appendChild(btn);
      }
      if (broken) {
        const btn = document.createElement("button");
        btn.className = "mini";
        btn.style.borderColor = "var(--bad)";
        btn.textContent = "load broken";
        btn.addEventListener("click", () => loadLesson(broken));
        div.appendChild(btn);
      }
      el.appendChild(div);
    }
    if (!stageNums.length) {
      const empty = document.createElement("div");
      empty.className = "d";
      empty.textContent = "no lessons in harness/lessons/ yet.";
      el.appendChild(empty);
    }
  }

  // The panel shows: whatever check row the user clicked (teachFocusCode),
  // else the first thing actually wrong with the CURRENT design (a fresh
  // check, not stale state) — so it works for any netlist, not just a
  // lesson loaded by name, and updates the instant an edit fixes it.
  function renderTeachPanel() {
    const el = document.getElementById("teachPanel");
    if (!teachMode) { el.style.display = "none"; el.innerHTML = ""; return; }
    el.style.display = "";

    let code = teachFocusCode;
    if (!code) {
      let res = null;
      try { res = callChecks(nl); } catch (e) { res = null; }
      const firstBad = res && !res.error ? res.checks.find(c => !c.pass || c.tier === "warn") : null;
      code = firstBad ? firstBad.code : null;
    }
    if (!code) {
      el.innerHTML = `<div style="color:var(--dim)">This design is currently clean — nothing to explain yet. Load a stage on the left, or click any check row below once something doesn't pass.</div>`;
      return;
    }
    let exp;
    try { exp = callExplainError(code); } catch (e) { exp = { error: String(e) }; }
    if (exp.error) {
      el.innerHTML = `<div style="color:var(--dim)">${exp.error}</div>`;
      return;
    }
    el.innerHTML =
      `<div style="font-weight:700;color:var(--accent);letter-spacing:0.08em">${exp.code}</div>` +
      `<div style="margin-top:5px"><b style="color:var(--dim)">what</b> ${exp.what}</div>` +
      `<div style="margin-top:5px"><b style="color:var(--dim)">why</b> ${exp.why}</div>` +
      `<div style="margin-top:5px"><b style="color:var(--dim)">fix</b> ${exp.fix}</div>`;
  }

  document.getElementById("teachBtn").addEventListener("click", () => { teachMode ? exitTeachMode() : enterTeachMode(); });
