//! What the plan is turned into for a person or for a simulator: a report to print, the list of the parts that are missing, and a G-code that only moves the head (for a simulator of
//! the movements; it has no commands for the pump, the valves or the nozzle, because those depend on the machine and nothing here has run on one).

use crate::feeding::Picks;
use crate::parts::Part;
use crate::planner::AdvancedPlan;

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// A report for a person: the totals, the batches, and what is missing or low.
pub fn report(plan: &AdvancedPlan, picks: &Picks, undefined: &[(String, usize)]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} parts in {} batches, order {}, {} tip changes, {} hard-order groups\n",
        plan.plan.placements.len(),
        plan.plan.batches,
        plan.order.name(),
        plan.tip_changes,
        plan.tiers
    ));
    out.push_str(&format!(
        "travel {:.1} mm, about {:.1} s\n",
        plan.plan.travel_mm, plan.plan.seconds
    ));
    if plan.align_s > 0.0 {
        out.push_str(&format!(
            "of which {:.1} s for the camera to look at the parts\n",
            plan.align_s
        ));
    }
    if plan.approach_s > 0.0 {
        out.push_str(&format!(
            "of which {:.1} s for the head to go past the places and come back from the positive side\n",
            plan.approach_s
        ));
    }
    let mut batch = usize::MAX;
    for (p, d) in plan.plan.placements.iter().zip(&plan.details) {
        if p.batch != batch {
            batch = p.batch;
            out.push_str(&format!("batch {}\n", batch + 1));
        }
        out.push_str(&format!(
            "  nozzle {} {} from {} to X {:.3} Y {:.3} turn {:.1} (rank {}, tip '{}')\n",
            p.nozzle + 1,
            p.reference,
            p.feeder,
            p.x,
            p.y,
            p.rotation,
            d.rank,
            d.tip
        ));
    }
    for r in &picks.without_feeder {
        out.push_str(&format!("no feeder for {r}\n"));
    }
    for (id, n) in &picks.short {
        out.push_str(&format!("feeder {id} is short of {n} parts\n"));
    }
    for (id, n) in &picks.low {
        out.push_str(&format!("feeder {id} will have {n} parts left\n"));
    }
    for (kind, n) in undefined {
        out.push_str(&format!(
            "not described in the tables: {kind} ({n} parts)\n"
        ));
    }
    out
}

/// The parts that the table marks as not to be mounted, for the report: one line with how many there are and their references, and nothing when there are none.
pub fn dnp_text(dnp: &[Part]) -> String {
    if dnp.is_empty() {
        return String::new();
    }
    let refs: Vec<&str> = dnp.iter().map(|p| p.reference.as_str()).collect();
    format!(
        "not to be mounted, left out of the plan: {} parts ({})\n",
        dnp.len(),
        refs.join(", ")
    )
}

/// The parts that are missing, as a CSV: `kind,what,detail` with one line for each part without a feeder, each feeder that is short and each kind that no table describes.
pub fn missing_csv(picks: &Picks, undefined: &[(String, usize)]) -> String {
    let mut out = String::from("kind,what,detail\n");
    for r in &picks.without_feeder {
        out.push_str(&format!("no_feeder,{},\n", csv_field(r)));
    }
    for (id, n) in &picks.short {
        out.push_str(&format!("short,{},{n}\n", csv_field(id)));
    }
    for (kind, n) in undefined {
        out.push_str(&format!("undefined,{},{n}\n", csv_field(kind)));
    }
    out
}

/// A G-code that only moves the head, for a simulator of the movements: for each batch the moves to the pick points and then to the places, with the rest in comments.
pub fn gcode(plan: &AdvancedPlan) -> String {
    gcode_with_runout(plan, 0.0, 0.0)
}

/// The same G-code with the runout of the nozzle tip taken into account: when `runout_mm` is above 0 each place is moved by the opposite of where the tip is with respect to the axis of
/// the nozzle at the angle of the part (`phase_deg` is the direction of the tip at 0 degrees), so that the tip lands on the place. With 0 it is exactly `gcode`.
pub fn gcode_with_runout(plan: &AdvancedPlan, runout_mm: f64, phase_deg: f64) -> String {
    let mut out = String::from(
        "; simulation of the movements only: no command for the pump, the valves or the nozzle\n; units are millimetres and degrees\nG21\nG90\n",
    );
    if runout_mm > 0.0 {
        out.push_str(&format!(
            "; the places are moved for a nozzle tip {runout_mm:.4} mm from its axis towards {phase_deg:.3} degrees at 0 degrees\n"
        ));
    }
    let placements = &plan.plan.placements;
    let mut i = 0;
    while i < placements.len() {
        let batch = placements[i].batch;
        let mut j = i;
        while j < placements.len() && placements[j].batch == batch {
            j += 1;
        }
        out.push_str(&format!("; batch {}\n", batch + 1));
        // the head picks every part of the batch, then places them in the same order
        for p in &placements[i..j] {
            out.push_str(&format!(
                "; pick {} from {} with nozzle {}\nG0 X{:.3} Y{:.3}\n",
                p.reference,
                p.feeder,
                p.nozzle + 1,
                p.pick.0,
                p.pick.1
            ));
        }
        for p in &placements[i..j] {
            let (dx, dy) = crate::calibrate::offset_of(runout_mm, phase_deg, p.rotation);
            out.push_str(&format!(
                "; place {} turned {:.1}\nG0 X{:.3} Y{:.3}\n",
                p.reference,
                p.rotation,
                p.x - dx,
                p.y - dy
            ));
        }
        i = j;
    }
    out.push_str("; end\n");
    out
}
