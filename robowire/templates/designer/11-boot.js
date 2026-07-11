// designer/11-boot.js — event wiring + startup — original execution order preserved
  cv.addEventListener("pointerdown", e => {
    const mx = e.offsetX, my = e.offsetY;
    const pin = pinAt(mx, my);
    if (pin) {
      if (pickHandler) { const h = pickHandler; pickHandler = null; h(pin); refresh(); return; }
      // Press starts a potential drag-wire; release decides (drag onto a
      // pin to connect, or a plain click for the click-click flow).
      wireDrag = { from: pin, cur: [mx, my], moved: false };
      cv.setPointerCapture(e.pointerId);
      draw();
      return;
    }
    // removal badge on the selected box? (2D only; Delete works everywhere)
    if (mode === "2d" && selInst && layout[selInst]) {
      const g = boxGeo(selInst);
      if (Math.hypot(mx - (g.x + g.w / 2 - 2), my - (g.y - g.h / 2 + 2)) < 10) {
        removeInstance(selInst);
        refresh(); return;
      }
    }
    const wi = wireAt(mx, my);
    if (wi >= 0) {
      selNet = wi; selInst = null;
      renderNets(); renderSelInfo(); draw();
      const row = document.querySelectorAll("#nets .row")[wi];
      if (row) row.scrollIntoView({ block: "nearest" });
      return;
    }
    const inst = instAt(mx, my);
    if (inst) {
      selInst = inst; selNet = -1;
      if (mode === "2d") {
        dragInst = inst; dragMoved = false;
        dragOff = [mx - layout[inst][0], my - layout[inst][1]];
        cv.setPointerCapture(e.pointerId);
      }
      renderNets(); renderSelInfo(); draw();
      return;
    }
    if (mode === "3d") {
      orbiting = true;
      lastXY = [mx, my];
      cv.classList.add("dragging");
      cv.setPointerCapture(e.pointerId);
      return;
    }
    pending = null; selInst = null; selNet = -1;
    renderNets(); renderSelInfo(); draw();
  });
  cv.addEventListener("pointermove", e => {
    if (wireDrag) {
      wireDrag.cur = [e.offsetX, e.offsetY];
      if (!wireDrag.moved) {
        const sp = mode === "3d" ? (pin3(wireDrag.from) && project3(pin3(wireDrag.from))) : pinXY(wireDrag.from);
        if (sp && Math.hypot(e.offsetX - sp[0], e.offsetY - sp[1]) > 7) wireDrag.moved = true;
      }
      draw();
      return;
    }
    if (orbiting) {
      yaw += (e.offsetX - lastXY[0]) * 0.008;
      pitch = Math.max(0.05, Math.min(1.45, pitch + (e.offsetY - lastXY[1]) * 0.006));
      lastXY = [e.offsetX, e.offsetY];
      draw();
      return;
    }
    if (dragInst) {
      dragMoved = true;
      layout[dragInst] = [e.offsetX - dragOff[0], e.offsetY - dragOff[1]];
      draw();
      return;
    }
    // cursor feedback + pin hover explanation
    const mx = e.offsetX, my = e.offsetY;
    const hoverPin = pinAt(mx, my);
    cv.style.cursor = hoverPin ? "crosshair"
      : wireAt(mx, my) >= 0 ? "pointer"
      : instAt(mx, my) ? "move" : "default";
    if (hoverPin) {
      const nets = nl.nets.filter(n => n.pins.includes(hoverPin)).map(n => n.id);
      for (const bus of nl.buses) {
        if (bus.sda === hoverPin || bus.scl === hoverPin) nets.push(bus.id);
        for (const d of bus.devices) if (d.xshut === hoverPin) nets.push("XSHUT " + d.inst);
      }
      hint(`${hoverPin} — ${pinProse(hoverPin) || "?"}${nets.length ? " · on " + nets.join(", ") : " · unwired"}`);
    }
  });
  cv.addEventListener("pointerup", e => {
    if (wireDrag) {
      const target = pinAt(e.offsetX, e.offsetY);
      if (wireDrag.moved && target && target !== wireDrag.from) {
        createNet(wireDrag.from, target);
        pending = null; wireDrag = null;
        refresh(); return;
      }
      if (!wireDrag.moved) {
        const pin = wireDrag.from;
        if (!pending) pending = pin;
        else if (pending !== pin) { createNet(pending, pin); pending = null; wireDrag = null; refresh(); return; }
        else pending = null;
      }
      wireDrag = null;
      draw(); return;
    }
    if (dragInst) { dragInst = null; saveLayout(); }
    if (orbiting) { orbiting = false; cv.classList.remove("dragging"); }
  });
  cv.addEventListener("wheel", e => {
    if (mode !== "3d") return;
    e.preventDefault();
    dist = Math.max(250, Math.min(2600, dist * (1 + e.deltaY * 0.001)));
    draw();
  }, { passive: false });
  document.getElementById("arrangeBtn").addEventListener("click", autoArrange);
  document.getElementById("modeBtn").addEventListener("click", () => {
    mode = mode === "2d" ? "3d" : "2d";
    document.getElementById("modeBtn").textContent = mode === "2d" ? "3D view" : "2D view";
    pending = null;
    draw();
  });
  cv.addEventListener("dragover", e => {
    if (e.dataTransfer.types.includes("text/robowire-part")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    }
  });
  cv.addEventListener("drop", e => {
    const partId = e.dataTransfer.getData("text/robowire-part");
    if (!partId) return;
    e.preventDefault();
    placeInstance(partId, e.offsetX, e.offsetY);
    refresh();
  });
  window.addEventListener("keydown", e => {
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    if (e.key === "Escape") { pending = null; pickHandler = null; wireDrag = null; selInst = null; selNet = -1; renderNets(); renderSelInfo(); draw(); hint(""); }
    if ((e.key === "r" || e.key === "R") && selInst && mode === "2d") {
      const L = layout[selInst];
      L[2] = ((L[2] || 0) + 90) % 360;
      saveLayout();
      draw();
      return;
    }
    if ((e.key === "Delete" || e.key === "Backspace")) {
      if (selNet >= 0) { nl.nets.splice(selNet, 1); selNet = -1; refresh(); }
      else if (selInst) { removeInstance(selInst); refresh(); }
      e.preventDefault();
    }
  });
  document.getElementById("partFilter").addEventListener("input", renderPalette);

  document.getElementById("exportBtn").addEventListener("click", () => {
    document.getElementById("ioTitle").textContent = "netlist json — canonical (layout not included)";
    document.getElementById("ioText").value = JSON.stringify(nl, null, 2);
    document.getElementById("ioApply").style.display = "none";
    dlg.showModal();
  });
  document.getElementById("importBtn").addEventListener("click", () => {
    document.getElementById("ioTitle").textContent = "paste netlist json";
    document.getElementById("ioText").value = "";
    document.getElementById("ioApply").style.display = "";
    dlg.showModal();
  });
  document.getElementById("ioApply").addEventListener("click", () => {
    try {
      nl = JSON.parse(document.getElementById("ioText").value);
      if (!nl.failsafe) nl.failsafe = { rx_loss: "", stop_pins: [] };
      if (!nl.buses) nl.buses = [];
      layout = {};
      try { layout = JSON.parse(localStorage.getItem(LAYOUT_KEY()) || "{}"); } catch {}
      dlg.close(); refresh();
    } catch (e) { alert("parse error: " + e.message); }
  });
  document.getElementById("ioDownload").addEventListener("click", () => {
    const blob = new Blob([JSON.stringify(nl, null, 2) + "\n"], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = nl.name + ".json";
    a.click();
  });
  document.getElementById("ioClose").addEventListener("click", () => dlg.close());
  window.addEventListener("resize", resize);
  if (typeof ResizeObserver !== "undefined") {
    new ResizeObserver(() => resize()).observe(cv);
  }
  resize();
  refresh();
  // First-ever load (no saved layout): arrange instead of the overlap
  // cascade, once the grid has settled.
  requestAnimationFrame(() => {
    if (!hadSavedLayout && Object.keys(nl.instances).length) autoArrange();
  });
