// designer/09-panels.js — sidebar panels: layers, examples, palette, nets, buses, failsafe, selection card, checks
  function renderLayers() {
    const el = document.getElementById("layerToggles");
    el.innerHTML = "";
    const iso = document.createElement("label");
    iso.className = "ck";
    iso.style.marginBottom = "6px";
    const isoInp = document.createElement("input");
    isoInp.type = "checkbox"; isoInp.checked = isolate;
    isoInp.addEventListener("change", () => { isolate = isoInp.checked; draw(); });
    const isoTag = document.createElement("span");
    isoTag.style.cssText = "color:var(--accent);font-weight:700";
    isoTag.textContent = "isolate";
    iso.append(isoInp, isoTag, document.createTextNode(" selection + its wiring"));
    el.appendChild(iso);

    const wiresHd = document.createElement("div");
    wiresHd.style.cssText = "color:var(--dim);font-size:9px;letter-spacing:0.14em;text-transform:uppercase;margin:4px 0 2px";
    wiresHd.textContent = "wires";
    el.appendChild(wiresHd);
    for (const [k, label] of Object.entries(LAYER_LABELS)) {
      const lab = document.createElement("label");
      lab.className = "ck";
      const inp = document.createElement("input");
      inp.type = "checkbox"; inp.checked = layerOn[k];
      inp.addEventListener("change", () => { layerOn[k] = inp.checked; draw(); });
      const sw = document.createElement("span");
      sw.style.cssText = "width:14px;height:3px;border-radius:2px;display:inline-block;background:" + (COLORS[k] || "#999");
      lab.append(inp, sw, document.createTextNode(label));
      el.appendChild(lab);
    }
    const compHd = document.createElement("div");
    compHd.style.cssText = "color:var(--dim);font-size:9px;letter-spacing:0.14em;text-transform:uppercase;margin:8px 0 2px";
    compHd.textContent = "components";
    el.appendChild(compHd);
    const presentKinds = [...new Set(PARTS.filter(p => p.elec && Object.keys(p.elec.pins || {}).length).map(p => p.kind))];
    for (const k of presentKinds) {
      const label = KIND_LABELS[k] || k;
      if (kindOn[k] === undefined) kindOn[k] = true;
      const lab = document.createElement("label");
      lab.className = "ck";
      const inp = document.createElement("input");
      inp.type = "checkbox"; inp.checked = kindOn[k];
      inp.addEventListener("change", () => {
        kindOn[k] = inp.checked;
        if (selInst && !inp.checked && kindOf(selInst) === k) { selInst = null; }
        draw();
      });
      const sw = document.createElement("span");
      sw.style.cssText = "width:14px;height:10px;border:1px solid #77848c;border-radius:2px;display:inline-block";
      lab.append(inp, sw, document.createTextNode(label));
      el.appendChild(lab);
    }
  }

  // Shared wire geometry so drawing and hit-testing can't disagree. Each
  // endpoint gets a short perpendicular lead stub out of its pin, so wires
  // visibly plug INTO a pin instead of swooping past its neighbours.
  function renderSelInfo() {
    const el = document.getElementById("selinfo");
    if (selInst && nl.instances[selInst]) {
      const part = partById[nl.instances[selInst]];
      const prefix = selInst + ".";
      const nets = nl.nets.filter(n => n.pins.some(pn => pn.startsWith(prefix)));
      const busRefs = nl.buses.filter(b =>
        b.devices.some(d => d.inst === selInst) || b.sda.startsWith(prefix) || b.scl.startsWith(prefix));
      el.style.display = "block";
      el.innerHTML =
        `<b>${selInst}</b> <span class="chip" style="background:#8b969b">${part.kind}</span>` +
        `<div class="about">${part.description || "(no catalogue description)"}</div>` +
        `<div class="meta">${part.id} · ${part.mass_g ?? "?"} g${part.provisional ? " · datasheet-provisional" : ""}</div>` +
        (nets.length || busRefs.length
          ? `<ul>` + nets.map(n =>
              `<li><span style="color:${COLORS[netClass(n)] || "#999"}">●</span> ${n.id} → ` +
              n.pins.filter(pn => !pn.startsWith(prefix)).join(" · ") + `</li>`).join("") +
            busRefs.map(b => `<li><span style="color:${COLORS.i2c}">●</span> ${b.id} (sensor bus)</li>`).join("") +
            `</ul>`
          : `<div class="meta">not wired yet</div>`);
    } else if (selNet >= 0 && nl.nets[selNet]) {
      const net = nl.nets[selNet];
      const cls = netClass(net);
      el.style.display = "block";
      const np = netProse(net.id);
      el.innerHTML =
        `<b>${net.id}</b> <span class="chip" style="background:${COLORS[cls] || "#999"}">${LAYER_LABELS[cls] || cls}</span>` +
        `<div class="about">${np.about}</div>` +
        `<ul>` + (np.ends.length ? np.ends : net.pins).map(x => `<li>· ${x}</li>`).join("") + `</ul>`;
    } else {
      el.style.display = "none";
      el.innerHTML = "";
    }
  }

  const ARRANGE_ROWS = [
    ["battery", "connector", "fuse", "ptc", "switch", "regulator"],
    ["esc", "motor", "servo"],
    ["mcu"],
    ["tof", "imu", "radio"],
    ["led", "resistor", "potentiometer", "buzzer", "button"],
  ];
  function autoArrange() {
    const insts = Object.keys(nl.instances);
    if (!insts.length) return;
    const rowOf = inst => {
      const k = kindOf(inst);
      const idx = ARRANGE_ROWS.findIndex(r => r.includes(k));
      return idx >= 0 ? idx : ARRANGE_ROWS.length;
    };
    const byRow = new Map();
    for (const inst of insts.sort()) {
      const r = rowOf(inst);
      if (!byRow.has(r)) byRow.set(r, []);
      byRow.get(r).push(inst);
    }
    // Connectivity graph, weighted per net by 1/(members-1): a net with many
    // members (ground, most often) touches almost everything, so its pull on
    // any ONE neighbor is diffuse and shouldn't dominate — letting it do so
    // is exactly what turns into a long diagonal wire crossing unrelated
    // boxes. A net with just two members (the common case: one specific
    // rail between exactly these two parts) pulls at full strength, since
    // THAT connection is the one actually worth keeping short and aligned.
    const adj = new Map(insts.map(i => [i, new Map()]));
    const link = (a, b, weight) => {
      if (a === b || !adj.has(a) || !adj.has(b)) return;
      adj.get(a).set(b, (adj.get(a).get(b) || 0) + weight);
      adj.get(b).set(a, (adj.get(b).get(a) || 0) + weight);
    };
    for (const net of nl.nets) {
      const is = [...new Set(net.pins.map(pn => splitPin(pn)[0]))];
      const weight = 1 / Math.max(1, is.length - 1);
      for (let i = 0; i < is.length; i++) for (let j = i + 1; j < is.length; j++) link(is[i], is[j], weight);
    }
    for (const bus of nl.buses) {
      const m = splitPin(bus.sda)[0];
      const fanout = 1 + bus.devices.length;
      const weight = 1 / Math.max(1, fanout - 1);
      for (const d of bus.devices) {
        link(m, d.inst, weight);
        if (d.xshut) link(splitPin(d.xshut)[0], d.inst, weight);
      }
    }
    const cw = cv.clientWidth || 900;
    const SP = 185;
    const placedX = {};
    let y = 100;
    const rows = [...byRow.keys()].sort((a, b) => a - b);
    rows.forEach((r, ri) => {
      let members = byRow.get(r);
      if (ri > 0) {
        // Weighted barycenter: sit each component under the connection-
        // weighted average x of whatever's already placed above it.
        members = members
          .map(inst => {
            let wsum = 0, xsum = 0;
            for (const [n, w] of adj.get(inst) || []) {
              if (placedX[n] === undefined) continue;
              wsum += w; xsum += w * placedX[n];
            }
            const bx = wsum > 0 ? xsum / wsum : cw / 2;
            return { inst, bx };
          })
          .sort((a, b) => a.bx - b.bx || (a.inst < b.inst ? -1 : 1))
          .map(o => o.inst);
      }
      let maxH = 60;
      for (const inst of members) maxH = Math.max(maxH, boxGeo(inst).h);
      members.forEach((inst, ci) => {
        const rot = (layout[inst] && layout[inst][2]) || 0;
        const x = cw / 2 + (ci - (members.length - 1) / 2) * SP;
        layout[inst] = [x, y + maxH / 2, rot];
        placedX[inst] = x;
      });
      y += maxH + 85;
    });
    saveLayout();
    draw();
  }
  const PART_GROUPS = [
    ["power", ["battery", "switch", "connector", "fuse", "ptc"]],
    ["drive", ["esc", "motor", "servo"]],
    ["brain", ["mcu"]],
    ["sensors", ["tof", "imu"]],
    ["radio", ["radio"]],
    ["indicators & passives", ["led", "buzzer", "resistor", "potentiometer", "button"]],
  ];
  function renderPalette() {
    const el = document.getElementById("palette");
    el.innerHTML = "";
    const q = (document.getElementById("partFilter").value || "").toLowerCase();
    const electrical = PARTS.filter(p => p.elec && Object.keys(p.elec.pins || {}).length);
    const grouped = new Set();
    const matches = p => !q || p.id.includes(q) || p.kind.includes(q) || (p.description || "").toLowerCase().includes(q);
    const renderGroup = (label, parts) => {
      if (!parts.length) return;
      const hd = document.createElement("div");
      hd.style.cssText = "color:var(--dim);font-size:9px;letter-spacing:0.14em;text-transform:uppercase;margin:8px 0 3px";
      hd.textContent = label;
      el.appendChild(hd);
      for (const p of parts) renderCard(p);
    };
    for (const [label, kinds] of PART_GROUPS) {
      const parts = electrical.filter(p => kinds.includes(p.kind) && matches(p));
      parts.forEach(p => grouped.add(p.id));
      renderGroup(label, parts);
    }
    renderGroup("other", electrical.filter(p => !grouped.has(p.id) && matches(p)));
    function renderCard(p) {
      const div = document.createElement("div");
      div.className = "part";
      div.innerHTML = `<span>${p.id}<br><span class="k">${p.kind}</span></span>`;
      div.draggable = true;
      div.title = (p.description || "") + " — drag onto the board, or click + place";
      div.addEventListener("dragstart", e => {
        e.dataTransfer.setData("text/robowire-part", p.id);
        e.dataTransfer.effectAllowed = "copy";
      });
      const btn = document.createElement("button");
      btn.className = "mini"; btn.textContent = "+ place";
      btn.addEventListener("click", () => { placeInstance(p.id); refresh(); });
      div.appendChild(btn);
      el.appendChild(div);
    }
  }
  function renderExamples() {
    const el = document.getElementById("examples");
    el.innerHTML = "";
    // Worked examples first; broken-on-purpose lessons clearly quarantined.
    const worked = EXAMPLES.filter(ex => !ex.name.startsWith("lesson-"));
    const lessons = EXAMPLES.filter(ex => ex.name.startsWith("lesson-"));
    const card = (ex, lesson) => {
      const div = document.createElement("div");
      div.className = "part";
      if (lesson) div.style.borderColor = "var(--bad)";
      div.innerHTML = `<span>${ex.name.replace(/^(example|lesson)-/, "")}` +
        (lesson
          ? `<br><span class="k" style="color:var(--bad);font-weight:700">⚠ BROKEN ON PURPOSE — a repair exercise</span>`
          : `<br><span class="k" style="color:var(--ok)">works — all checks pass</span>`);
      const btn = document.createElement("button");
      btn.className = "mini";
      btn.textContent = lesson ? "load broken" : "load";
      btn.addEventListener("click", () => {
        if (Object.keys(nl.instances).length && !confirm("Replace the current design with '" + ex.name + "'?")) return;
        nl = JSON.parse(JSON.stringify(ex));
        if (!nl.failsafe) nl.failsafe = { rx_loss: "", stop_pins: [] };
        if (!nl.buses) nl.buses = [];
        layout = {};
        selInst = null; selNet = -1; pending = null;
        refresh();
        autoArrange();
        if (lesson) {
          hint("⚠ this circuit is broken on purpose — read the FAIL in the checks panel, then repair it (hint: the fixed version is in the examples list)");
        }
      });
      div.appendChild(btn);
      el.appendChild(div);
    };
    for (const ex of worked) card(ex, false);
    if (lessons.length) {
      const hd = document.createElement("div");
      hd.style.cssText = "color:var(--bad);font-size:9px;letter-spacing:0.14em;text-transform:uppercase;margin:8px 0 3px";
      hd.textContent = "repair exercises (broken on purpose)";
      el.appendChild(hd);
      for (const ex of lessons) card(ex, true);
    }
  }

  function renderNets() {
    const el = document.getElementById("nets");
    el.innerHTML = "";
    nl.nets.forEach((net, i) => {
      const div = document.createElement("div");
      div.className = "row" + (i === selNet ? " sel" : "");
      const col = COLORS[netClass(net)] || "#999";
      div.innerHTML =
        `<div class="hd"><span style="width:10px;height:10px;border-radius:2px;background:${col};display:inline-block"></span>` +
        `<input class="id" value="${net.id}"> V:<input class="volts" value="${net.volts ?? ""}" placeholder="—">` +
        ` sig:<select><option value="">—</option><option${net.signal === "pwm" ? " selected" : ""}>pwm</option><option${net.signal === "uart" ? " selected" : ""}>uart</option></select>` +
        ` <button class="mini del">✕</button></div>` +
        `<div class="pins">${net.pins.join(" · ")} <button class="mini addpin">+pin</button></div>`;
      div.addEventListener("click", () => { if (selNet !== i) { selNet = i; selInst = null; renderNets(); renderSelInfo(); draw(); } });
      div.querySelector(".id").addEventListener("change", e => { net.id = e.target.value; refresh(); });
      div.querySelector(".volts").addEventListener("change", e => {
        const v = parseFloat(e.target.value);
        if (isNaN(v)) delete net.volts; else net.volts = v;
        refresh();
      });
      div.querySelector("select").addEventListener("change", e => {
        if (e.target.value) net.signal = e.target.value; else delete net.signal;
        refresh();
      });
      div.querySelector(".del").addEventListener("click", e => { e.stopPropagation(); clearWireBend(net.id); nl.nets.splice(i, 1); selNet = -1; refresh(); });
      div.querySelector(".addpin").addEventListener("click", e => {
        e.stopPropagation();
        hint("pick a pin to add to " + net.id);
        pickHandler = pin => { if (!net.pins.includes(pin)) net.pins.push(pin); };
      });
      el.appendChild(div);
    });
  }

  function pinSelectBtn(current, onpick) {
    const b = document.createElement("button");
    b.className = "mini";
    b.textContent = current || "pick pin";
    b.addEventListener("click", () => {
      hint("pick a pin…");
      pickHandler = pin => { onpick(pin); };
    });
    return b;
  }

  function renderBuses() {
    const el = document.getElementById("buses");
    el.innerHTML = "";
    nl.buses.forEach((bus, bi) => {
      const div = document.createElement("div");
      div.className = "row";
      const hd = document.createElement("div");
      hd.className = "hd";
      hd.innerHTML = `<b>${bus.id}</b> (${bus.kind}) sda:${bus.sda} scl:${bus.scl} `;
      const del = document.createElement("button"); del.className = "mini"; del.textContent = "✕";
      del.addEventListener("click", () => { nl.buses.splice(bi, 1); refresh(); });
      hd.appendChild(del);
      div.appendChild(hd);
      bus.devices.forEach((dev, di) => {
        const d = document.createElement("div");
        d.className = "pins";
        d.innerHTML = `${dev.inst} @${dev.addr}` +
          (dev.reassign_to ? ` → ${dev.reassign_to}` : "") +
          (dev.xshut ? ` (xshut ${dev.xshut})` : "") + " ";
        const rm = document.createElement("button"); rm.className = "mini"; rm.textContent = "✕";
        rm.addEventListener("click", () => { bus.devices.splice(di, 1); refresh(); });
        d.appendChild(rm);
        div.appendChild(d);
      });
      const add = document.createElement("button");
      add.className = "mini"; add.textContent = "+ device";
      add.addEventListener("click", () => {
        const eligible = Object.entries(nl.instances).filter(([, pid]) => partById[pid]?.elec?.bus);
        if (!eligible.length) { alert("no bus-capable instances placed"); return; }
        const inst = prompt("instance (" + eligible.map(e => e[0]).join(", ") + "):", eligible[0][0]);
        if (!inst || !(inst in nl.instances)) return;
        const part = partById[nl.instances[inst]];
        const dev = { inst, addr: part?.elec?.bus?.default_addr || "0x00" };
        const re = prompt("reassign to address (blank = keep " + dev.addr + "):", "");
        if (re) dev.reassign_to = re;
        bus.devices.push(dev);
        if (re || part?.elec?.bus?.requires_xshut) {
          hint("pick the MCU pin driving " + inst + ".XSHUT");
          pickHandler = pin => { dev.xshut = pin; refresh(); };
        }
        refresh();
      });
      div.appendChild(add);
      el.appendChild(div);
    });
    document.getElementById("addBus").onclick = () => {
      const bus = { id: "i2c" + nl.buses.length, kind: "i2c", sda: "", scl: "", devices: [] };
      nl.buses.push(bus);
      hint("pick the SDA pin (an MCU i2c_sda-capable pin)");
      pickHandler = pin => {
        bus.sda = pin;
        hint("pick the SCL pin");
        pickHandler = p2 => { bus.scl = p2; refresh(); };
        draw();
      };
      refresh();
    };
  }

  function renderFailsafe() {
    const ta = document.getElementById("fsloss");
    ta.value = nl.failsafe.rx_loss || "";
    ta.onchange = () => { nl.failsafe.rx_loss = ta.value; refresh(); };
    const el = document.getElementById("fspins");
    el.innerHTML = "stop pins: " + (nl.failsafe.stop_pins.join(" · ") || "—") + " ";
    const b = document.createElement("button");
    b.className = "mini"; b.textContent = "+ stop pin";
    b.addEventListener("click", () => {
      hint("pick a stop pin (e.g. the ESC signal inputs)");
      pickHandler = pin => {
        if (!nl.failsafe.stop_pins.includes(pin)) nl.failsafe.stop_pins.push(pin);
      };
    });
    el.appendChild(b);
    const clr = document.createElement("button");
    clr.className = "mini"; clr.textContent = "clear";
    clr.addEventListener("click", () => { nl.failsafe.stop_pins = []; refresh(); });
    el.appendChild(clr);
  }

  let checkTimer = null;
  function runChecksSoon() {
    clearTimeout(checkTimer);
    checkTimer = setTimeout(() => {
      updateProse();
      const res = callChecks(nl);
      const verdict = document.getElementById("verdict");
      const el = document.getElementById("checks");
      if (res.error) {
        verdict.className = "bad";
        verdict.textContent = "SCHEMA: " + res.error;
        el.innerHTML = "";
        return;
      }
      const fails = res.checks.filter(c => !c.pass);
      verdict.className = fails.length ? "bad" : "ok";
      verdict.textContent = fails.length
        ? `E-CHECK FAILURES (${fails.length}) — do not solder`
        : "HARNESS LEGAL — all checks pass";
      el.innerHTML = res.checks.map(c => {
        const warn = c.pass && c.tier === "warn";
        const col = !c.pass ? "var(--bad)" : warn ? "var(--accent)" : "var(--ok)";
        const label = !c.pass ? "FAIL" : warn ? "WARN" : "PASS";
        const clickable = teachMode ? ' style="cursor:pointer"' : "";
        return `<div class="check" data-code="${c.code}"${clickable}><span class="pill" style="background:${col}">${label}</span>` +
          `<span>${c.code}</span><span class="d">${c.detail}</span></div>`;
      }).join("");
      // Teaching mode: any check row (not just the currently-loaded lesson's
      // own code) can be clicked to read its what/why/fix explanation —
      // rebuilt fresh each tick, same as the rest of this panel, since a
      // click here is a discrete action, not a gesture mid-drag (unlike the
      // run panel's sliders, nothing here needs identity preserved).
      if (teachMode) {
        el.querySelectorAll(".check").forEach(row => {
          row.addEventListener("click", () => {
            teachFocusCode = row.dataset.code;
            renderTeachPanel();
          });
        });
      }
    }, 250);
  }

  function refresh() {
    nameEl.value = nl.name;
    renderLayers();
    if (teachMode) renderTeachLessons(); else renderExamples();
    renderPalette();
    renderNets();
    renderBuses();
    renderFailsafe();
    renderSelInfo();
    renderTeachPanel();
    draw();
    runChecksSoon();
  }
