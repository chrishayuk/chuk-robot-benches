// designer/01-state.js — model + ui state (netlist, layout, selection, layers)
  const partById = {};
  for (const p of PARTS) partById[p.id] = p;
  let nl = START_NETLIST || { name: "new-harness", instances: {}, nets: [], buses: [], failsafe: { rx_loss: "", stop_pins: [] } };
  if (!nl.failsafe) nl.failsafe = { rx_loss: "", stop_pins: [] };
  if (!nl.buses) nl.buses = [];
  const LAYOUT_KEY = () => "robowire-layout:" + nl.name;
  let layout = {};
  try { layout = JSON.parse(localStorage.getItem(LAYOUT_KEY()) || "{}"); } catch {}
  const hadSavedLayout = Object.keys(layout).length > 0;
  function saveLayout() { try { localStorage.setItem(LAYOUT_KEY(), JSON.stringify(layout)); } catch {} }

  const COLORS = {
    vbat: "#e05c50", v5: "#e8a33d", v33: "#c3c94f", gnd: "#7d8b93",
    motor: "#e0784f", pwm: "#4f9dd6", uart: "#a97fd6", i2c: "#57b48f", sig: "#9aa4ab",
  };
  const decl = ep => {
    const [inst, pin] = splitPin(ep);
    return partById[nl.instances[inst]]?.elec?.pins?.[pin] || null;
  };
  const splitPin = ep => { const i = ep.indexOf("."); return [ep.slice(0, i), ep.slice(i + 1)]; };
  function netClass(net) {
    const isGnd = net.volts == null && net.signal == null &&
      net.pins.some(p => p.endsWith(".GND") || p.endsWith(".-"));
    if (isGnd) return "gnd";
    if (net.pins.some(p => { const d = decl(p); return d && (d.role === "motor_in" || d.role === "motor_out"); })) return "motor";
    if (net.volts != null) return net.volts > 6 ? "vbat" : net.volts > 4 ? "v5" : "v33";
    if (net.signal === "pwm") return "pwm";
    if (net.signal === "uart") return "uart";
    return "sig";
  }

  const cv = document.getElementById("board");
  const cx = cv.getContext("2d");
  let dpr = 1;
  let mode = "2d";
  let wireDrag = null;      // {from, cur:[x,y], moved} while dragging a wire
  let dragWireNet = null;   // net index while dragging an EXISTING wire's bend point
  let dragWireMoved = false;
  let dragWireZ = 0;        // 3D only: the fixed world-space height the drag slides across
  let orbiting = false;
  // Run mode (12-run.js) — declared here, not there: draw()/draw3() (07/08)
  // read runMode/runState unconditionally, and 11-boot.js's startup tail
  // calls draw() before module 12 has run, so these must be live before
  // that (function declarations hoist fully; `let` does not).
  let runMode = false;
  let runInputs = { switches: {}, buttons: {}, throttles: {}, sensor_values: {} };
  let runState = { nets: {}, instances: {} };
  let heldButtonInst = null;
  let spinPhase = 0;
  let spinRAF = null;
  let pending = null;       // first pin clicked in wire mode
  let pickHandler = null;   // active pin-pick flow (bus/failsafe forms)
  let selNet = -1;
  let selInst = null;
  let dragInst = null, dragOff = [0, 0], dragMoved = false;
  const LAYER_LABELS = {
    vbat: "battery power", v5: "5V supply", v33: "3.3V supply", gnd: "ground",
    motor: "motor power", pwm: "drive commands", uart: "radio link", i2c: "sensor bus", sig: "other signals",
  };
  const layerOn = {};
  for (const k of Object.keys(LAYER_LABELS)) layerOn[k] = true;
  const KIND_LABELS = {
    battery: "battery", switch: "switch", esc: "motor controller", mcu: "brain (MCU)",
    motor: "motors", tof: "floor sensors", imu: "motion sensor", radio: "radio", wiring: "loom allowance",
  };
  const kindOn = {};
  for (const k of Object.keys(KIND_LABELS)) kindOn[k] = true;
  let isolate = false;
  function kindOf(inst) { return partById[nl.instances[inst]]?.kind || "other"; }
  function connectedTo(a) {
    const set = new Set([a]);
    const prefix = a + ".";
    for (const net of nl.nets) {
      if (net.pins.some(p => p.startsWith(prefix))) {
        for (const p of net.pins) set.add(splitPin(p)[0]);
      }
    }
    for (const bus of nl.buses) {
      const onBus = bus.devices.some(d => d.inst === a) ||
        bus.sda.startsWith(prefix) || bus.scl.startsWith(prefix) ||
        bus.devices.some(d => d.xshut && d.xshut.startsWith(prefix));
      if (onBus) {
        set.add(splitPin(bus.sda)[0]);
        for (const d of bus.devices) {
          set.add(d.inst);
          if (d.xshut) set.add(splitPin(d.xshut)[0]);
        }
      }
    }
    return set;
  }
  function focusSet() {
    if (!isolate) return null;
    if (selInst) return connectedTo(selInst);
    if (selNet >= 0 && nl.nets[selNet]) {
      const set = new Set();
      for (const p of nl.nets[selNet].pins) set.add(splitPin(p)[0]);
      return set;
    }
    return null;
  }
  let currentFocus = null; // recomputed per draw
  function instVisible(inst) {
    if (kindOn[kindOf(inst)] === false) return false;
    if (currentFocus && !currentFocus.has(inst)) return false;
    return true;
  }
  let lastXY = [0, 0];
