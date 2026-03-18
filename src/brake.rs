use std::fmt;

use crate::error::Error;

const BRAKE_SCALE: u32 = 1_000_000;
const BACKGROUND_FRACTION: f64 = 0.1;

/// Built-in execution brakes.
///
/// `Stop` fully pauses a thread until it is moved away.
/// `Background` uses a low built-in brake level.
/// `Custom` accepts any validated CPU fraction in `(0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brake {
  Stop,
  Background,
  Custom(BrakeValue),
}

/// Validated CPU fraction used by [`Brake::Custom`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BrakeValue(u32);

impl Brake {
  /// Unrestricted runnable work.
  pub const fn full() -> Self {
    Self::Custom(BrakeValue::FULL)
  }

  /// Create a validated custom brake level in `(0.0, 1.0]`.
  pub fn custom(cpu_fraction: f64) -> Result<Self, Error> {
    Ok(Self::Custom(BrakeValue::new(cpu_fraction)?))
  }

  pub(crate) fn cpu_fraction(self) -> Option<f64> {
    match self {
      Self::Stop => None,
      Self::Background => Some(BACKGROUND_FRACTION),
      Self::Custom(value) => Some(value.as_f64()),
    }
  }

  pub(crate) fn is_stopped(self) -> bool {
    matches!(self, Self::Stop)
  }
}

impl BrakeValue {
  pub const FULL: Self = Self(BRAKE_SCALE);

  pub fn new(cpu_fraction: f64) -> Result<Self, Error> {
    if !cpu_fraction.is_finite() {
      return Err(Error::InvalidBrake("fraction must be finite".into()));
    }
    if !(0.0 < cpu_fraction && cpu_fraction <= 1.0) {
      return Err(Error::InvalidBrake("fraction must be greater than 0.0 and at most 1.0".into()));
    }

    let scaled = (cpu_fraction * BRAKE_SCALE as f64).round() as u32;
    Ok(Self(scaled.clamp(1, BRAKE_SCALE)))
  }

  pub fn as_f64(self) -> f64 {
    self.0 as f64 / BRAKE_SCALE as f64
  }

  #[cfg(target_os = "linux")]
  pub(crate) const fn raw(self) -> u32 {
    self.0
  }
}

impl TryFrom<f64> for BrakeValue {
  type Error = Error;

  fn try_from(value: f64) -> Result<Self, Self::Error> {
    Self::new(value)
  }
}

impl From<BrakeValue> for Brake {
  fn from(value: BrakeValue) -> Self {
    Self::Custom(value)
  }
}

impl fmt::Debug for BrakeValue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "BrakeValue({:.6})", self.as_f64())
  }
}
