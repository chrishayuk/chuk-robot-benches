// designer/02-prose.js — plain-English mirrors of the Rust prose generator (flagged duplication)
  const KIND_NOUN = {
    battery: "the battery pack", switch: "the power switch", esc: "the motor controller",
    mcu: "the brain", radio: "the radio receiver", tof: "a floor sensor",
    imu: "the motion sensor", motor: "a drive motor", wiring: "the loom allowance",
  };
  function who(inst) {
    return (KIND_NOUN[kindOf(inst)] || "'" + inst + "'") + " (" + inst + ")";
  }
  function roleText(d) {
    if (!d) return "";
    switch (d.role) {
      case "pos": return "battery positive terminal";
      case "gnd": return "ground";
      case "power_in": return d.v_range ? `power input (rated ${d.v_range[0]}–${d.v_range[1]} V)` : "power input";
      case "power_out": return `regulator output (${d.volts} V)`;
      case "switch_in": return "master switch input";
      case "switch_out": return "master switch output";
      case "motor_in": return "motor terminal";
      case "motor_out": return "driver output" + (d.channel ? ` (channel ${d.channel})` : "");
      case "signal_in": return "signal input" + (d.signal ? ` (${d.signal})` : "");
      case "signal_out": return "signal output" + (d.signal ? ` (${d.signal})` : "");
      case "mcu_io": return `MCU pin [${(d.caps || []).join(", ")}]`;
      case "bus_sda": return "I2C data (SDA)";
      case "bus_scl": return "I2C clock (SCL)";
      case "gpio_in": return "control input";
      default: return d.role;
    }
  }
  function wireAbout(net) {
    const cls = netClass(net);
    const insts = [...new Set(net.pins.map(p => splitPin(p)[0]))];
    const listWho = skipKind => insts
      .filter(i => kindOf(i) !== skipKind)
      .map(who)
      .join(" and ");
    switch (cls) {
      case "vbat":
        return `The robot's main power line: raw ${net.volts ?? 7.4} V battery power flowing between ` +
          insts.map(who).join(" and ") + ". Nothing downstream runs without it — and E40 demands the master switch sits in this path.";
      case "v5":
        return "The 5-volt supply: the motor controller's built-in regulator (BEC) makes clean 5 V and feeds " +
          (listWho("esc") || "its loads") + " — this is what keeps them alive.";
      case "v33":
        return "The 3.3-volt sensor supply: the brain's onboard regulator powers " +
          (listWho("mcu") || "the sensors") + " through this line.";
      case "gnd":
        return "The shared ground return. Every component's current flows back to the battery through this — it is the other half of every circuit on the robot.";
      case "motor": {
        const m = insts.find(i => kindOf(i) === "motor");
        return "Motor power: the controller pushes current down this wire to spin " +
          (m ? who(m) : "the motor") + ". Reverse the current and the wheel reverses.";
      }
      case "pwm":
        return "A drive command line: the brain sets one motor channel's speed by sending timed pulses (PWM) down this wire. On radio loss the failsafe holds it at neutral — that is what stops the robot (E41).";
      case "uart":
        return "The control link: the radio receiver streams the driver's stick positions to the brain over this wire. If those frames stop arriving, the brain knows the radio is gone and triggers the failsafe.";
      default:
        return "A signal line.";
    }
  }
