//! The simulated machine. It has a virtual clock that only moves when something takes time (a move, a pick, a place, a feed), so a job of an hour is simulated in a blink and
//! always gives the same times; and it can fail: each action fails with a chance taken from the configuration, from a generator with a seed, or at the place in the sequence that a
//! test chooses. The same seed and the same actions give the same failures, always.

use crate::config::{MachineConfig, VisionMode};
use crate::error::{Code, PnpError};
use crate::model::{Actuator, Camera, Driver, FeederDevice, Machine, Nozzle, Offset};
use crate::motion::{move_time, AxisMotion};
use crate::vision::{measure_image, render, VisionConfig};

pub const MAX_EVENTS: usize = 1_000_000;

/// A small generator of numbers (xorshift64*), the same on every platform.
#[derive(Debug, Clone, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // the seed is mixed so that 0 and 1 give very different streams, and the state is never 0
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
    }

    pub fn from_state(state: u64) -> Rng {
        Rng(if state == 0 {
            0x2545_F491_4F6C_DD1D
        } else {
            state
        })
    }

    pub fn state(&self) -> u64 {
        self.0
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number from 0 up to, not including, 1.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The three actions that can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimOp {
    Feed,
    Pick,
    Place,
}

impl SimOp {
    fn index(self) -> usize {
        match self {
            SimOp::Feed => 0,
            SimOp::Pick => 1,
            SimOp::Place => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SimOp::Feed => "feed",
            SimOp::Pick => "pick",
            SimOp::Place => "place",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub t_us: u64,
    pub text: String,
}

fn fault(op: SimOp, n: u64) -> PnpError {
    PnpError::new(
        Code::SimFault,
        format!("the {} number {n} was made to fail", op.name()),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimMachine {
    clock_us: u64,
    rng: Rng,
    rates: [f64; 3],
    /// A further, independent chance of failure for the next actions of each kind (set by the job from the kind of part and the feeder).
    extra: [f64; 3],
    force: Vec<(SimOp, u64)>,
    counts: [u64; 3],
    motion: (AxisMotion, AxisMotion),
    limits: (f64, f64, f64, f64),
    pos: (f64, f64),
    nozzles: Vec<Option<String>>,
    tip: String,
    pick_s: f64,
    place_s: f64,
    tip_change_s: f64,
    pub events: Vec<Event>,
}

impl SimMachine {
    pub fn from_config(cfg: &MachineConfig) -> Result<SimMachine, PnpError> {
        Ok(SimMachine {
            clock_us: 0,
            rng: Rng::new(cfg.sim_seed),
            rates: [0.0, cfg.sim_pick_fail_rate, cfg.sim_place_fail_rate],
            extra: [0.0; 3],
            force: Vec::new(),
            counts: [0; 3],
            motion: (
                AxisMotion::new(cfg.x_speed_mm_s, cfg.x_accel_mm_s2, cfg.x_jerk_mm_s3)?,
                AxisMotion::new(cfg.y_speed_mm_s, cfg.y_accel_mm_s2, cfg.y_jerk_mm_s3)?,
            ),
            limits: (
                cfg.limit_x_min_mm,
                cfg.limit_x_max_mm,
                cfg.limit_y_min_mm,
                cfg.limit_y_max_mm,
            ),
            pos: (0.0, 0.0),
            nozzles: vec![None; cfg.nozzles],
            tip: String::new(),
            pick_s: cfg.pick_s,
            place_s: cfg.place_s,
            tip_change_s: cfg.tip_change_s,
            events: Vec::new(),
        })
    }

    /// Makes the `nth` pick or place (counted from 1, from the start of the simulation) fail, whatever the chance says. A feed is made to fail on the feeder (`SimFeeder::force_fail`).
    pub fn force_fail(&mut self, op: SimOp, nth: u64) {
        self.force.push((op, nth));
    }

    /// Sets a further chance (0 to 1), independent of the general one, that the next actions of that kind fail: the chance of failing is then one minus the chance that neither
    /// fails. The job sets it before each pick and each place from the kind of part and the feeder; 0 takes it away.
    pub fn set_extra_fail(&mut self, op: SimOp, rate: f64) {
        self.extra[op.index()] = if rate.is_finite() {
            rate.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn note(&mut self, text: impl Into<String>) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(Event {
                t_us: self.clock_us,
                text: text.into(),
            });
        }
    }

    fn advance(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.clock_us = self.clock_us.saturating_add((seconds * 1e6).round() as u64);
        }
    }

    /// Whether the next action of that kind fails. A number is drawn every time, so that a change of the chance of one kind does not move the failures of the others.
    fn draw(&mut self, op: SimOp) -> Option<PnpError> {
        let i = op.index();
        self.counts[i] += 1;
        let n = self.counts[i];
        let r = self.rng.next_f64();
        let rate = if self.extra[i] > 0.0 {
            1.0 - (1.0 - self.rates[i]) * (1.0 - self.extra[i])
        } else {
            self.rates[i]
        };
        if self.force.contains(&(op, n)) || r < rate {
            Some(fault(op, n))
        } else {
            None
        }
    }

    /// The state that a saved job keeps: the generator and the counts of the actions.
    pub fn save(&self) -> (u64, [u64; 3], u64) {
        (self.rng.state(), self.counts, self.clock_us)
    }

    /// Puts back what `save` gave, and where the head is and what the nozzles hold.
    pub fn restore(
        &mut self,
        saved: (u64, [u64; 3], u64),
        pos: (f64, f64),
        holding: Vec<Option<String>>,
        tip: String,
    ) {
        self.rng = Rng::from_state(saved.0);
        self.counts = saved.1;
        self.clock_us = saved.2;
        self.pos = pos;
        if holding.len() == self.nozzles.len() {
            self.nozzles = holding;
        }
        self.tip = tip;
    }

    /// Puts a nozzle tip on at the start, without the time of a change.
    pub fn mount_tip(&mut self, tip: &str) {
        self.tip = tip.to_string();
        self.note(format!("nozzle tip '{tip}' mounted"));
    }

    pub fn nozzle_count(&self) -> usize {
        self.nozzles.len()
    }
}

impl Machine for SimMachine {
    fn now_us(&self) -> u64 {
        self.clock_us
    }

    fn position(&self) -> (f64, f64) {
        self.pos
    }

    fn move_to(&mut self, to: (f64, f64)) -> Result<(), PnpError> {
        let (x0, x1, y0, y1) = self.limits;
        if !(to.0.is_finite() && to.1.is_finite())
            || to.0 < x0
            || to.0 > x1
            || to.1 < y0
            || to.1 > y1
        {
            return Err(PnpError::new(
                Code::UnsafeSetting,
                format!(
                    "the move to X {} Y {} is outside the limits of the machine",
                    to.0, to.1
                ),
            ));
        }
        let t = move_time(
            to.0 - self.pos.0,
            to.1 - self.pos.1,
            &self.motion.0,
            &self.motion.1,
        );
        self.advance(t);
        self.pos = to;
        self.note(format!("move to X {:.3} Y {:.3}", to.0, to.1));
        Ok(())
    }

    fn wait(&mut self, seconds: f64) {
        self.advance(seconds);
    }
}

impl Nozzle for SimMachine {
    fn holding(&self, nozzle: usize) -> Option<&str> {
        self.nozzles.get(nozzle).and_then(|n| n.as_deref())
    }

    fn pick(&mut self, nozzle: usize, reference: &str) -> Result<(), PnpError> {
        let busy = match self.nozzles.get(nozzle) {
            None => {
                return Err(PnpError::new(
                    Code::PlanSetting,
                    format!("the machine has no nozzle {}", nozzle + 1),
                ))
            }
            Some(h) => h.clone(),
        };
        if let Some(held) = busy {
            return Err(PnpError::new(
                Code::NozzleBusy,
                format!("nozzle {} already holds {held}", nozzle + 1),
            ));
        }
        self.advance(self.pick_s);
        if let Some(e) = self.draw(SimOp::Pick) {
            self.note(format!("pick of {reference} failed"));
            return Err(e);
        }
        self.nozzles[nozzle] = Some(reference.to_string());
        self.note(format!("nozzle {} picked {reference}", nozzle + 1));
        Ok(())
    }

    fn place(&mut self, nozzle: usize) -> Result<String, PnpError> {
        let held = match self.nozzles.get(nozzle) {
            None => {
                return Err(PnpError::new(
                    Code::PlanSetting,
                    format!("the machine has no nozzle {}", nozzle + 1),
                ))
            }
            Some(None) => {
                return Err(PnpError::new(
                    Code::NozzleEmpty,
                    format!("nozzle {} holds nothing", nozzle + 1),
                ))
            }
            Some(Some(h)) => h.clone(),
        };
        self.advance(self.place_s);
        if let Some(e) = self.draw(SimOp::Place) {
            self.note(format!("place of {held} failed"));
            return Err(e);
        }
        self.nozzles[nozzle] = None;
        self.note(format!("nozzle {} placed {held}", nozzle + 1));
        Ok(held)
    }

    fn discard(&mut self, nozzle: usize) {
        if let Some(slot) = self.nozzles.get_mut(nozzle) {
            let had = slot.take();
            if let Some(h) = had {
                self.note(format!("nozzle {} discarded {h}", nozzle + 1));
            }
        }
    }

    fn tip(&self) -> &str {
        &self.tip
    }

    fn change_tip(&mut self, tip: &str) {
        self.advance(self.tip_change_s);
        self.tip = tip.to_string();
        self.note(format!("nozzle tip changed to '{tip}'"));
    }
}

/// A feeder of the simulator, with its own generator, so that its failures do not depend on what the other feeders do.
#[derive(Debug, Clone, PartialEq)]
pub struct SimFeeder {
    id: String,
    remaining: u32,
    enabled: bool,
    rate: f64,
    rng: Rng,
    attempts: u64,
    force: Vec<u64>,
}

impl SimFeeder {
    pub fn new(id: &str, remaining: u32, seed: u64, index: usize, rate: f64) -> SimFeeder {
        SimFeeder {
            id: id.to_string(),
            remaining,
            enabled: true,
            rate,
            rng: Rng::new(seed ^ ((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))),
            attempts: 0,
            force: Vec::new(),
        }
    }

    /// Makes the `nth` feed (counted from 1) fail.
    pub fn force_fail(&mut self, nth: u64) {
        self.force.push(nth);
    }

    pub fn save(&self) -> (u64, u64) {
        (self.rng.state(), self.attempts)
    }

    pub fn restore(&mut self, saved: (u64, u64), remaining: u32, enabled: bool) {
        self.rng = Rng::from_state(saved.0);
        self.attempts = saved.1;
        self.remaining = remaining;
        self.enabled = enabled;
    }
}

impl FeederDevice for SimFeeder {
    fn id(&self) -> &str {
        &self.id
    }

    fn remaining(&self) -> u32 {
        self.remaining
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    fn feed(&mut self) -> Result<(), PnpError> {
        self.attempts += 1;
        let r = self.rng.next_f64();
        if self.force.contains(&self.attempts) || r < self.rate {
            return Err(fault(SimOp::Feed, self.attempts));
        }
        Ok(())
    }

    fn take(&mut self) {
        self.remaining = self.remaining.saturating_sub(1);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimActuator {
    on: bool,
}

impl Actuator for SimActuator {
    fn set(&mut self, on: bool) -> Result<(), PnpError> {
        self.on = on;
        Ok(())
    }

    fn is_on(&self) -> bool {
        self.on
    }
}

/// A camera that sees every part displaced by the same offset (zero by default), less what has been corrected already, plus a random error of each reading; it can fail to give a
/// reading, by chance (with a generator of its own, so that nothing else in the simulation moves when it is used) or at a look that a test chooses. With `vision = pipeline` it does not
/// report the offset: it draws the picture of the part where the part is (`vision::render`, with a noise that depends on the seed and the number of the look) and measures that picture
/// with the vision pipeline, so a look can also fail because the pipeline finds no part, a spot of a wrong area or several spots (`PHOENIX-PNP-037` to `039`).
#[derive(Debug, Clone, PartialEq)]
pub struct SimCamera {
    /// How far, and how much turned, every part is from the nozzle before any correction.
    pub bias: Offset,
    /// The random error of a reading, from minus to plus this on each axis (millimetres) and, in degrees, on the turn.
    pub noise_mm: f64,
    /// The chance (0 to 1) that a look gives no reading.
    pub fail_rate: f64,
    applied: Offset,
    rng: Rng,
    looks: u64,
    force: Vec<u64>,
    /// The settings of the vision pipeline when the camera measures a picture; `None` when it reports the offset directly.
    vision: Option<VisionConfig>,
    /// The seed of the noise of the pictures; the picture of a look is drawn from this and the number of the look.
    image_seed: u64,
}

impl Default for SimCamera {
    fn default() -> SimCamera {
        SimCamera {
            bias: Offset::default(),
            noise_mm: 0.0,
            fail_rate: 0.0,
            applied: Offset::default(),
            rng: Rng::new(1),
            looks: 0,
            force: Vec::new(),
            vision: None,
            image_seed: 1,
        }
    }
}

impl SimCamera {
    /// A camera that sees every part displaced by `bias`, with no error in its readings and no failure.
    pub fn new(bias: Offset) -> SimCamera {
        SimCamera {
            bias,
            ..SimCamera::default()
        }
    }

    /// The camera that the configuration describes.
    pub fn from_config(cfg: &MachineConfig) -> SimCamera {
        SimCamera {
            bias: Offset {
                dx_mm: cfg.sim_align_dx_mm,
                dy_mm: cfg.sim_align_dy_mm,
                rotation_deg: cfg.sim_align_deg,
            },
            noise_mm: cfg.sim_align_noise_mm,
            fail_rate: cfg.sim_camera_fail_rate,
            rng: Rng::new(cfg.sim_seed ^ 0xA11C_E5ED_0000_0001),
            vision: (cfg.vision == VisionMode::Pipeline).then(VisionConfig::default),
            image_seed: cfg.sim_seed ^ 0x51CA_7E57_0000_0002,
            ..SimCamera::default()
        }
    }

    /// Gives the pipeline its settings. A camera that reports the offset directly (`vision = off`) is not changed.
    pub fn set_vision(&mut self, settings: VisionConfig) {
        if self.vision.is_some() {
            self.vision = Some(settings);
        }
    }

    /// Whether the camera measures a picture.
    pub fn uses_vision(&self) -> bool {
        self.vision.is_some()
    }

    /// Makes the `nth` look (counted from 1) give no reading.
    pub fn force_fail(&mut self, nth: u64) {
        self.force.push(nth);
    }

    /// A new part is under the camera: nothing has been corrected yet.
    pub fn clear(&mut self) {
        self.applied = Offset::default();
    }

    /// The machine has corrected the part by `correction`: the next look finds that much less.
    pub fn apply(&mut self, correction: Offset) {
        self.applied.dx_mm += correction.dx_mm;
        self.applied.dy_mm += correction.dy_mm;
        self.applied.rotation_deg += correction.rotation_deg;
    }

    /// The state that a saved job keeps: the generator and the number of looks.
    pub fn save(&self) -> (u64, u64) {
        (self.rng.state(), self.looks)
    }

    pub fn restore(&mut self, saved: (u64, u64)) {
        self.rng = Rng::from_state(saved.0);
        self.looks = saved.1;
    }
}

impl Camera for SimCamera {
    fn measure(&mut self, _nozzle: usize) -> Result<Offset, PnpError> {
        self.looks += 1;
        // four numbers are drawn at every look, so that the chance of failing does not move the errors of the readings
        let chance = self.rng.next_f64();
        let noise = [
            self.rng.next_f64(),
            self.rng.next_f64(),
            self.rng.next_f64(),
        ];
        if self.force.contains(&self.looks) || chance < self.fail_rate {
            return Err(PnpError::new(
                Code::SimFault,
                format!("the camera look number {} was made to fail", self.looks),
            ));
        }
        if let Some(settings) = &self.vision {
            // the picture is of the part where it is now: where it was, less what has been corrected
            let truth = Offset {
                dx_mm: self.bias.dx_mm - self.applied.dx_mm,
                dy_mm: self.bias.dy_mm - self.applied.dy_mm,
                rotation_deg: self.bias.rotation_deg - self.applied.rotation_deg,
            };
            let mut noise_rng =
                Rng::new(self.image_seed ^ self.looks.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let picture = render(settings, truth, &mut noise_rng);
            return measure_image(settings, &picture).map(|reading| reading.offset);
        }
        let spread = |r: f64| (2.0 * r - 1.0) * self.noise_mm;
        Ok(Offset {
            dx_mm: self.bias.dx_mm - self.applied.dx_mm + spread(noise[0]),
            dy_mm: self.bias.dy_mm - self.applied.dy_mm + spread(noise[1]),
            rotation_deg: self.bias.rotation_deg - self.applied.rotation_deg + spread(noise[2]),
        })
    }
}

/// A driver that answers `ok` to any line of a sane length, and refuses the others.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimDriver {
    pub sent: Vec<String>,
}

impl Driver for SimDriver {
    fn send(&mut self, line: &str) -> Result<String, PnpError> {
        let l = line.trim();
        if l.is_empty() || l.chars().count() > 256 || l.chars().any(char::is_control) {
            return Err(PnpError::new(
                Code::DriverError,
                "the line is empty, longer than 256 characters or has a control character",
            ));
        }
        self.sent.push(l.to_string());
        Ok("ok".to_string())
    }
}
