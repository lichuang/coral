//! Document values.
//!
//! [`CoralValue`] is the unified value type used throughout Coral.
//! It mirrors JSON semantics while adding a `Container` variant that
//! allows one CRDT container to reference another (e.g. a Map whose
//! value is a List).

use super::ContainerID;
use rustc_hash::FxHashMap;

/// A JSON-like value type with an additional `Container` variant.
///
/// `CoralValue` is what users read and write when interacting with
/// CRDT containers. Maps use [`FxHashMap`] for fast lookups.
///
/// # Equality
///
/// Only `PartialEq` is implemented because `F64` may hold `NaN`,
/// which does not satisfy reflexive equality.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CoralValue {
  /// JSON `null`.
  #[default]
  Null,
  /// Boolean value.
  Bool(bool),
  /// 64-bit signed integer.
  I64(i64),
  /// 64-bit floating point (IEEE 754).
  ///
  /// Note: `NaN != NaN`, which is why `CoralValue` does not implement `Eq`.
  F64(f64),
  /// UTF-8 string.
  String(String),
  /// Ordered list of values.
  List(Vec<CoralValue>),
  /// A map of string keys to values.
  ///
  /// Uses [`FxHashMap`] for O(1) average-case lookups, matching Loro's
  /// implementation.
  Map(FxHashMap<String, CoralValue>),
  /// A reference to a nested CRDT container.
  ///
  /// This variant appears when a container (e.g. a Map) stores another
  /// container (e.g. a List) as one of its values.
  Container(ContainerID),
}

impl CoralValue {
  /// Serializes the value to a JSON string.
  pub fn to_json(&self) -> Result<String, serde_json::Error> {
    let json_value = self.to_serde_value();
    serde_json::to_string(&json_value)
  }

  /// Deserializes a JSON string into a `CoralValue`.
  pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
    let json_value: serde_json::Value = serde_json::from_str(s)?;
    Ok(Self::from_serde_value(json_value))
  }

  /// Returns `true` if the value is a collection type (`List` or `Map`).
  pub const fn is_collection(&self) -> bool {
    matches!(self, Self::List(_) | Self::Map(_))
  }

  /// Returns `true` if the value is empty (empty string, list, or map).
  pub fn is_empty(&self) -> bool {
    match self {
      Self::String(s) => s.is_empty(),
      Self::List(v) => v.is_empty(),
      Self::Map(m) => m.is_empty(),
      _ => false,
    }
  }

  // -----------------------------------------------------------------------
  // Internal: conversion to/from serde_json::Value
  // -----------------------------------------------------------------------

  fn to_serde_value(&self) -> serde_json::Value {
    match self {
      Self::Null => serde_json::Value::Null,
      Self::Bool(b) => serde_json::Value::Bool(*b),
      Self::I64(n) => serde_json::Value::Number((*n).into()),
      Self::F64(n) => {
        serde_json::Value::Number(serde_json::Number::from_f64(*n).unwrap_or_else(|| 0.into()))
      }
      Self::String(s) => serde_json::Value::String(s.clone()),
      Self::List(v) => serde_json::Value::Array(v.iter().map(CoralValue::to_serde_value).collect()),
      Self::Map(m) => {
        let obj = m
          .iter()
          .map(|(k, v)| (k.clone(), v.to_serde_value()))
          .collect();
        serde_json::Value::Object(obj)
      }
      Self::Container(cid) => {
        // Containers are serialized as a special JSON object.
        serde_json::json!({"__coral_container": cid.to_string()})
      }
    }
  }

  fn from_serde_value(v: serde_json::Value) -> Self {
    match v {
      serde_json::Value::Null => Self::Null,
      serde_json::Value::Bool(b) => Self::Bool(b),
      serde_json::Value::Number(n) => {
        if let Some(i) = n.as_i64() {
          Self::I64(i)
        } else if let Some(f) = n.as_f64() {
          Self::F64(f)
        } else {
          Self::Null
        }
      }
      serde_json::Value::String(s) => Self::String(s),
      serde_json::Value::Array(a) => {
        Self::List(a.into_iter().map(Self::from_serde_value).collect())
      }
      serde_json::Value::Object(mut m) => {
        // Check for the special container marker.
        if let Some(serde_json::Value::String(s)) = m.remove("__coral_container")
          && let Ok(cid) = s.parse()
        {
          return Self::Container(cid);
        }
        Self::Map(
          m.into_iter()
            .map(|(k, v)| (k, Self::from_serde_value(v)))
            .collect(),
        )
      }
    }
  }
}

// Convenience constructors
impl CoralValue {
  /// Wraps a `bool`.
  pub fn bool(v: bool) -> Self {
    Self::Bool(v)
  }

  /// Wraps an `i64`.
  pub fn int(v: i64) -> Self {
    Self::I64(v)
  }

  /// Wraps an `f64`.
  pub fn float(v: f64) -> Self {
    Self::F64(v)
  }

  /// Wraps a `String`.
  pub fn string(v: impl Into<String>) -> Self {
    Self::String(v.into())
  }

  /// Wraps a `Vec<CoralValue>`.
  pub fn list(v: Vec<CoralValue>) -> Self {
    Self::List(v)
  }

  /// Wraps an [`FxHashMap<String, CoralValue>`].
  pub fn map(v: FxHashMap<String, CoralValue>) -> Self {
    Self::Map(v)
  }

  /// Wraps a [`ContainerID`].
  pub fn container(v: ContainerID) -> Self {
    Self::Container(v)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::ContainerType;

  #[test]
  fn test_coral_value_json_roundtrip() {
    let value = CoralValue::Map(FxHashMap::from_iter([
      ("name".to_string(), CoralValue::String("Alice".to_string())),
      ("age".to_string(), CoralValue::I64(30)),
      ("active".to_string(), CoralValue::Bool(true)),
    ]));
    let json = value.to_json().unwrap();
    let decoded = CoralValue::from_json(&json).unwrap();
    assert_eq!(value, decoded);
  }

  #[test]
  fn test_coral_value_list_json() {
    let value = CoralValue::List(vec![
      CoralValue::I64(1),
      CoralValue::I64(2),
      CoralValue::I64(3),
    ]);
    let json = value.to_json().unwrap();
    assert_eq!(json, "[1,2,3]");
  }

  #[test]
  fn test_coral_value_null_json() {
    let value = CoralValue::Null;
    let json = value.to_json().unwrap();
    assert_eq!(json, "null");
  }

  #[test]
  fn test_coral_value_container_json() {
    let cid = ContainerID::new_root("my_map", ContainerType::Map);
    let value = CoralValue::Container(cid);
    let json = value.to_json().unwrap();
    assert!(json.contains("__coral_container"));
    let decoded = CoralValue::from_json(&json).unwrap();
    assert_eq!(value, decoded);
  }

  #[test]
  fn test_coral_value_f64_no_eq() {
    // NaN != NaN is why we only implement PartialEq, not Eq.
    let a = CoralValue::F64(f64::NAN);
    let b = CoralValue::F64(f64::NAN);
    assert_ne!(a, b);
  }

  #[test]
  fn test_coral_value_convenience_constructors() {
    assert_eq!(CoralValue::bool(true), CoralValue::Bool(true));
    assert_eq!(CoralValue::int(42), CoralValue::I64(42));
    assert_eq!(CoralValue::float(3.14), CoralValue::F64(3.14));
    assert_eq!(
      CoralValue::string("hello"),
      CoralValue::String("hello".to_string())
    );
    assert_eq!(CoralValue::list(vec![]), CoralValue::List(vec![]));
    assert_eq!(
      CoralValue::map(FxHashMap::default()),
      CoralValue::Map(FxHashMap::default())
    );
  }

  #[test]
  fn test_coral_value_is_empty() {
    assert!(CoralValue::String("".to_string()).is_empty());
    assert!(CoralValue::List(vec![]).is_empty());
    assert!(CoralValue::Map(FxHashMap::default()).is_empty());
    assert!(!CoralValue::String("x".to_string()).is_empty());
    assert!(!CoralValue::I64(0).is_empty());
  }
}
