/// A value type representing the possible data stored in a Coral CRDT.
///
/// This is a minimal set of primitive types; more variants will be added
/// as the library grows (e.g., String, List, Map, Container).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
  /// The null value, used as the default.
  #[default]
  Null,
  /// A boolean value.
  Bool(bool),
  /// A 64-bit signed integer.
  I64(i64),
  /// A 64-bit floating-point number.
  Double(f64),
}

impl From<bool> for Value {
  fn from(v: bool) -> Self {
    Value::Bool(v)
  }
}

impl From<i64> for Value {
  fn from(v: i64) -> Self {
    Value::I64(v)
  }
}

impl From<f64> for Value {
  fn from(v: f64) -> Self {
    Value::Double(v)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_value_default_is_null() {
    let v: Value = Default::default();
    assert_eq!(v, Value::Null);
  }

  #[test]
  fn test_value_from_bool() {
    let v: Value = true.into();
    assert_eq!(v, Value::Bool(true));
  }

  #[test]
  fn test_value_from_i64() {
    let v: Value = 42i64.into();
    assert_eq!(v, Value::I64(42));
  }

  #[test]
  fn test_value_from_f64() {
    let v: Value = 3.14.into();
    assert_eq!(v, Value::Double(3.14));
  }
}
