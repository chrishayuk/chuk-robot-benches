// designer/05-edit.js — netlist mutations: create/remove, placement
  function createNet(a, b) {
    const net = { id: autoNetId(a, b), pins: [a, b] };
    const d1 = decl(a), d2 = decl(b);
    const src = [d1, d2].find(d => d && (d.role === "power_out"));
    if (src) net.volts = src.volts;
    else if ([d1, d2].some(d => d && d.role === "pos")) net.volts = 7.4;
    const sig = [d1, d2].find(d => d && d.signal);
    if (!net.volts && sig) net.signal = sig.signal === "crsf" ? "uart" : sig.signal;
    nl.nets.push(net);
    selNet = nl.nets.length - 1;
    selInst = null;
  }

  function removeInstance(inst) {
    delete nl.instances[inst];
    delete layout[inst];
    const prefix = inst + ".";
    for (const net of nl.nets) net.pins = net.pins.filter(p => !p.startsWith(prefix));
    for (const net of nl.nets) if (net.pins.length < 2) clearWireBend(net.id);
    nl.nets = nl.nets.filter(n => n.pins.length >= 2);
    for (const bus of nl.buses) {
      bus.devices = bus.devices.filter(d => d.inst !== inst);
      for (const dev of bus.devices) if (dev.xshut && dev.xshut.startsWith(prefix)) delete dev.xshut;
      if (bus.sda.startsWith(prefix)) bus.sda = "";
      if (bus.scl.startsWith(prefix)) bus.scl = "";
    }
    nl.buses = nl.buses.filter(b => b.sda && b.scl || b.devices.length);
    nl.failsafe.stop_pins = nl.failsafe.stop_pins.filter(p => !p.startsWith(prefix));
    selInst = null; selNet = -1;
    saveLayout();
  }

  function placeInstance(partId, x, y) {
    const p = partById[partId];
    const short = { battery: "batt", switch: "sw", radio: "rx", mcu: "mcu", esc: "esc", imu: "imu", motor: "m", tof: "tof" };
    const base = short[p.kind] || p.kind;
    let cand = base, k = 2;
    while (cand in nl.instances) cand = base + "_" + k++;
    nl.instances[cand] = partId;
    if (x != null) layout[cand] = [x, y];
    saveLayout();
    selInst = cand; selNet = -1;
    return cand;
  }
