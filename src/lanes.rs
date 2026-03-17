use std::fmt::Debug;
use std::hash::Hash;

/// Implement this on your enum to define execution lanes.
///
/// Each variant has a CPU budget from 0.0 (OS minimum) to 1.0 (unrestricted).
/// The manager sets up OS resources for each variant at construction time.
///
/// # Example
///
/// ```
/// use thread_lanes::Lanes;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// enum MyLanes { Hot, Warm, Cold }
///
/// impl Lanes for MyLanes {
///   fn cpu(&self) -> f64 {
///     match self {
///       Self::Hot  => 1.0,
///       Self::Warm => 0.1,
///       Self::Cold => 0.0,
///     }
///   }
///   fn all() -> &'static [Self] {
///     &[Self::Hot, Self::Warm, Self::Cold]
///   }
/// }
/// ```
pub trait Lanes: Debug + Clone + Copy + PartialEq + Eq + Hash + Send + Sync + 'static {
  /// CPU budget for this lane. 0.0 = as slow as the OS allows. 1.0 = unrestricted.
  /// Clamped to [0.0, 1.0].
  fn cpu(&self) -> f64;

  /// All variants. The manager sets up OS resources for each at construction.
  fn all() -> &'static [Self];
}
