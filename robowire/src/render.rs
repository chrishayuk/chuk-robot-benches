//! Schematic SVG render (spec §4.3): rails as horizontal trunks, ground
//! along the bottom, the I2C bus as a paired trunk, instances as pin boxes,
//! signal nets point-to-point. Deterministic layout, zero dependencies —
//! the output is the build sheet, not art.

use crate::catalogue::ElecCatalogue;
use crate::schema::{split_pin, Netlist};
use std::collections::BTreeMap;
use std::fmt::Write;

const BOX_W: f64 = 118.0;
const BOX_GAP: f64 = 26.0;
const PIN_STEP: f64 = 16.0;
const ROW_A_Y: f64 = 250.0;
const ROW_B_Y: f64 = 470.0;

fn net_color(volts: Option<f64>, signal: Option<&str>, is_gnd: bool) -> &'static str {
    if is_gnd {
        return "#556066";
    }
    if let Some(v) = volts {
        if v > 6.0 {
            return "#c94f3d";
        }
        if v > 4.0 {
            return "#d8913a";
        }
        return "#a3a13c";
    }
    match signal {
        Some("pwm") => "#3f7cc4",
        Some("uart") => "#8a5bb8",
        _ => "#7a8288",
    }
}

pub fn render_svg(nl: &Netlist, cat: &ElecCatalogue) -> Result<String, String> {
    // Row assignment: power/drive chain up top, logic below.
    let row_of = |kind: &str| -> u8 {
        match kind {
            "battery" | "switch" | "esc" | "motor" => 0,
            _ => 1,
        }
    };
    let mut row_a: Vec<&String> = Vec::new();
    let mut row_b: Vec<&String> = Vec::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if row_of(&part.kind) == 0 {
            row_a.push(inst);
        } else {
            row_b.push(inst);
        }
    }

    // Box geometry: x by slot, pins spread along top and bottom edges.
    struct BoxGeo {
        x: f64,
        y: f64,
        h: f64,
        pins: BTreeMap<String, (f64, f64)>, // pin -> stub point
    }
    let mut geo: BTreeMap<String, BoxGeo> = BTreeMap::new();
    for (row, insts, y) in [(0u8, &row_a, ROW_A_Y), (1, &row_b, ROW_B_Y)] {
        let _ = row;
        for (slot, inst) in insts.iter().enumerate() {
            let part_id = &nl.instances[**&inst];
            let (part, _) = cat.get(part_id)?;
            let used: Vec<String> = part
                .elec
                .as_ref()
                .map(|e| e.pins.keys().cloned().collect())
                .unwrap_or_default();
            let h = 30.0 + 6.0;
            let x = 30.0 + slot as f64 * (BOX_W + BOX_GAP);
            let mut pins = BTreeMap::new();
            // Stubs alternate top/bottom, spread deterministically.
            for (i, pin) in used.iter().enumerate() {
                let px = x + 14.0 + (i as f64 * PIN_STEP) % (BOX_W - 24.0);
                let on_top = i % 2 == 0;
                let py = if on_top { y } else { y + h };
                pins.insert(pin.clone(), (px, py));
            }
            geo.insert((*inst).clone(), BoxGeo { x, y, h, pins });
        }
    }

    let width = 30.0
        + (row_a.len().max(row_b.len()) as f64) * (BOX_W + BOX_GAP)
        + 30.0;
    let gnd_y = ROW_B_Y + 140.0;
    let height = gnd_y + 60.0;

    let mut s = String::new();
    write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="ui-monospace,Menlo,monospace" font-size="11">"##
    )
    .unwrap();
    s.push_str(r##"<rect width="100%" height="100%" fill="#f4f5f2"/>"##);
    write!(
        s,
        r##"<text x="30" y="24" font-size="15" fill="#1a2125" font-weight="bold">{} — harness schematic (robowire {})</text>"##,
        nl.name,
        crate::ROBOWIRE_VERSION
    )
    .unwrap();

    // Power rails (nets with volts), stacked from the top.
    let mut rail_y: BTreeMap<String, f64> = BTreeMap::new();
    let mut y = 56.0;
    for net in &nl.nets {
        if net.volts.is_some() {
            rail_y.insert(net.id.clone(), y);
            let color = net_color(net.volts, None, false);
            write!(
                s,
                r##"<line x1="30" y1="{y}" x2="{}" y2="{y}" stroke="{color}" stroke-width="2.5"/><text x="{}" y="{}" fill="{color}">{} ({}V)</text>"##,
                width - 30.0,
                width - 150.0,
                y - 4.0,
                net.id,
                net.volts.unwrap()
            )
            .unwrap();
            y += 22.0;
        }
    }
    // I2C trunks.
    let mut bus_lines: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for bus in &nl.buses {
        let (sda_y, scl_y) = (y, y + 14.0);
        bus_lines.insert(bus.id.clone(), (sda_y, scl_y));
        for (label, ly, dash) in [("SDA", sda_y, "none"), ("SCL", scl_y, "6 3")] {
            write!(
                s,
                r##"<line x1="30" y1="{ly}" x2="{}" y2="{ly}" stroke="#2e8b57" stroke-width="2" stroke-dasharray="{dash}"/><text x="{}" y="{}" fill="#2e8b57">{} {}</text>"##,
                width - 30.0,
                width - 150.0,
                ly - 3.0,
                bus.id,
                label
            )
            .unwrap();
        }
        y += 40.0;
    }
    // Ground rail at the bottom.
    write!(
        s,
        r##"<line x1="30" y1="{gnd_y}" x2="{}" y2="{gnd_y}" stroke="#556066" stroke-width="2.5"/><text x="{}" y="{}" fill="#556066">GND</text>"##,
        width - 30.0,
        width - 150.0,
        gnd_y - 4.0
    )
    .unwrap();

    // Wires. Vertical taps for rails/gnd/bus; elbows for point-to-point.
    let stub = |inst: &str, pin: &str| -> Option<(f64, f64)> {
        geo.get(inst).and_then(|g| g.pins.get(pin)).copied()
    };
    for net in &nl.nets {
        let is_gnd = net.volts.is_none()
            && net.signal.is_none()
            && net.pins.iter().any(|p| p.ends_with(".GND") || p.ends_with(".-"));
        let color = net_color(net.volts, net.signal.as_deref(), is_gnd);
        if let Some(ry) = rail_y.get(&net.id) {
            for p in &net.pins {
                let (inst, pin) = split_pin(p).unwrap();
                if let Some((px, py)) = stub(inst, pin) {
                    write!(s, r##"<line x1="{px}" y1="{py}" x2="{px}" y2="{ry}" stroke="{color}" stroke-width="1.6"/><circle cx="{px}" cy="{ry}" r="3" fill="{color}"/>"##).unwrap();
                }
            }
        } else if is_gnd {
            for p in &net.pins {
                let (inst, pin) = split_pin(p).unwrap();
                if let Some((px, py)) = stub(inst, pin) {
                    write!(s, r##"<line x1="{px}" y1="{py}" x2="{px}" y2="{gnd_y}" stroke="{color}" stroke-width="1.6"/><circle cx="{px}" cy="{gnd_y}" r="3" fill="{color}"/>"##).unwrap();
                }
            }
        } else {
            // Point-to-point: simple elbow through the midline between rows.
            let pts: Vec<(f64, f64)> = net
                .pins
                .iter()
                .filter_map(|p| {
                    let (inst, pin) = split_pin(p).ok()?;
                    stub(inst, pin)
                })
                .collect();
            if pts.len() == 2 {
                let ((x1, y1), (x2, y2)) = (pts[0], pts[1]);
                let midy = (ROW_A_Y + 36.0 + ROW_B_Y) / 2.0
                    + (x1.min(x2) / width) * 40.0; // deterministic de-overlap
                write!(s, r##"<polyline points="{x1},{y1} {x1},{midy} {x2},{midy} {x2},{y2}" fill="none" stroke="{color}" stroke-width="1.6"/>"##).unwrap();
                write!(
                    s,
                    r##"<text x="{}" y="{}" fill="{color}" font-size="9">{}</text>"##,
                    (x1 + x2) / 2.0 - 12.0,
                    midy - 3.0,
                    net.id
                )
                .unwrap();
            }
        }
    }
    // Bus taps.
    for bus in &nl.buses {
        let (sda_y, scl_y) = bus_lines[&bus.id];
        for (endpoint, ly) in [(&bus.sda, sda_y), (&bus.scl, scl_y)] {
            let (inst, pin) = split_pin(endpoint).unwrap();
            if let Some((px, py)) = stub(inst, pin) {
                write!(s, r##"<line x1="{px}" y1="{py}" x2="{px}" y2="{ly}" stroke="#2e8b57" stroke-width="1.6"/><circle cx="{px}" cy="{ly}" r="3" fill="#2e8b57"/>"##).unwrap();
            }
        }
        for dev in &bus.devices {
            for (pin, ly) in [("SDA", sda_y), ("SCL", scl_y)] {
                if let Some((px, py)) = stub(&dev.inst, pin) {
                    write!(s, r##"<line x1="{px}" y1="{py}" x2="{px}" y2="{ly}" stroke="#2e8b57" stroke-width="1.3"/><circle cx="{px}" cy="{ly}" r="2.5" fill="#2e8b57"/>"##).unwrap();
                }
            }
            if let (Some(x), Some((px, py))) = (&dev.xshut, stub(&dev.inst, "XSHUT")) {
                let (xi, xp) = split_pin(x).unwrap();
                if let Some((qx, qy)) = stub(xi, xp) {
                    let midy = (py.max(qy) + py.min(qy)) / 2.0 + 12.0;
                    write!(s, r##"<polyline points="{px},{py} {px},{midy} {qx},{midy} {qx},{qy}" fill="none" stroke="#2e8b57" stroke-width="1.2" stroke-dasharray="3 3"/>"##).unwrap();
                }
            }
            // Final address label under the device box.
            let addr = dev.reassign_to.as_ref().unwrap_or(&dev.addr);
            if let Some(g) = geo.get(&dev.inst) {
                write!(
                    s,
                    r##"<text x="{}" y="{}" fill="#2e8b57" font-size="9">@{addr}{}</text>"##,
                    g.x,
                    g.y + g.h + 24.0,
                    if dev.reassign_to.is_some() { " (reassigned)" } else { "" }
                )
                .unwrap();
            }
        }
    }

    // Boxes on top of wires.
    for (inst, g) in &geo {
        write!(
            s,
            r##"<rect x="{}" y="{}" width="{BOX_W}" height="{}" rx="4" fill="#ffffff" stroke="#8b969b"/><text x="{}" y="{}" fill="#1a2125" font-weight="bold">{}</text><text x="{}" y="{}" fill="#5c686d" font-size="9">{}</text>"##,
            g.x, g.y, g.h,
            g.x + 8.0, g.y + 14.0, inst,
            g.x + 8.0, g.y + 27.0, nl.instances[inst],
        )
        .unwrap();
        for (pin, (px, py)) in &g.pins {
            let above = (*py - g.y).abs() < 1e-9;
            write!(
                s,
                r##"<circle cx="{px}" cy="{py}" r="2.5" fill="#1a2125"/><text x="{}" y="{}" fill="#5c686d" font-size="8" transform="rotate(-45 {} {})">{pin}</text>"##,
                px + 2.0,
                if above { py - 5.0 } else { py + 11.0 },
                px + 2.0,
                if above { py - 5.0 } else { py + 11.0 },
            )
            .unwrap();
        }
    }
    s.push_str("</svg>");
    Ok(s)
}
