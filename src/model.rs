//! The parts of a machine as traits, without any input or output: a machine that moves, nozzles that hold a part, feeders that advance, an actuator, a camera under the nozzle and a
//! driver. The simulator in `sim` implements them; a real machine would implement them with a driver, and nothing in the planner or in the job would change.

use crate::error::PnpError;

/// What a camera under the nozzle measures of the part it holds: how far it is from where it should be, and how much it is turned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    pub dx_mm: f64,
    pub dy_mm: f64,
    pub rotation_deg: f64,
}

/// A machine that has a head that moves in the XY plane and a clock.
pub trait Machine {
    /// The time of the machine's clock, in microseconds.
    fn now_us(&self) -> u64;
    fn position(&self) -> (f64, f64);
    /// Moves the head to a place; the clock advances by the time of the move.
    fn move_to(&mut self, to: (f64, f64)) -> Result<(), PnpError>;
    /// Lets `seconds` go by.
    fn wait(&mut self, seconds: f64);
}

/// The nozzles of the head, numbered from 0.
pub trait Nozzle {
    fn holding(&self, nozzle: usize) -> Option<&str>;
    /// A nozzle that already holds a part cannot pick another.
    fn pick(&mut self, nozzle: usize, reference: &str) -> Result<(), PnpError>;
    /// A nozzle that holds nothing has nothing to place. Gives back the reference of the part placed.
    fn place(&mut self, nozzle: usize) -> Result<String, PnpError>;
    /// Gets rid of the part, whatever the machine thinks the nozzle holds.
    fn discard(&mut self, nozzle: usize);
    fn tip(&self) -> &str;
    fn change_tip(&mut self, tip: &str);
}

/// One feeder of the machine.
pub trait FeederDevice {
    fn id(&self) -> &str;
    fn remaining(&self) -> u32;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, on: bool);
    /// Advances the tape; it can fail.
    fn feed(&mut self) -> Result<(), PnpError>;
    /// One part has been taken from the feeder.
    fn take(&mut self);
}

/// A switch: a pump, a valve, a light.
pub trait Actuator {
    fn set(&mut self, on: bool) -> Result<(), PnpError>;
    fn is_on(&self) -> bool;
}

/// A camera that looks at the part on a nozzle.
pub trait Camera {
    fn measure(&mut self, nozzle: usize) -> Result<Offset, PnpError>;
}

/// The link to the controller of the machine: a line goes out, an answer comes back.
pub trait Driver {
    fn send(&mut self, line: &str) -> Result<String, PnpError>;
}
