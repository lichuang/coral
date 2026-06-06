/// The CRDT state of a counter container.
///
/// A simple running total. Since addition is commutative and associative,
/// concurrent increments from any peer can be applied directly without
/// per-peer tracking.
#[derive(Debug, Clone, Default)]
pub struct CounterState {
  value: f64,
}

impl CounterState {
  /// Creates a new counter state initialized to zero.
  pub fn new() -> Self {
    Self::default()
  }

  /// Applies an increment to the counter.
  pub fn apply_increment(&mut self, delta: f64) {
    self.value += delta;
  }

  /// Returns the current counter value.
  pub fn value(&self) -> f64 {
    self.value
  }
}
