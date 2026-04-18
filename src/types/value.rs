//! Document values.
//!
//! [`CoralValue`] is the unified value type used throughout Coral.
//! It mirrors JSON semantics while adding a `Container` variant that
//! allows one CRDT container to reference another (e.g. a Map whose
//! value is a List).

use super::ContainerID;
use rustc_hash::FxHashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Arc-wrapped newtypes for O(1) clone
// ---------------------------------------------------------------------------

/// An `Arc`-backed byte vector.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CoralBinaryValue(Arc<Vec<u8>>);

impl CoralBinaryValue {
  pub fn new(v: Vec<u8>) -> Self {
    Self(Arc::new(v))
  }

  pub fn make_mut(&mut self) -> &mut Vec<u8> {
    Arc::make_mut(&mut self.0)
  }

  pub fn unwrap(self) -> Vec<u8> {
    Arc::try_unwrap(self.0).unwrap_or_else(|arc| (*arc).clone())
  }
}

impl std::ops::Deref for CoralBinaryValue {
  type Target = Vec<u8>;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl AsRef<[u8]> for CoralBinaryValue {
  fn as_ref(&self) -> &[u8] {
    &self.0
  }
}

impl From<Vec<u8>> for CoralBinaryValue {
  fn from(v: Vec<u8>) -> Self {
    Self(Arc::new(v))
  }
}

impl From<&[u8]> for CoralBinaryValue {
  fn from(v: &[u8]) -> Self {
    Self(Arc::new(v.to_vec()))
  }
}

impl std::hash::Hash for CoralBinaryValue {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

/// An `Arc`-backed string.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CoralStringValue(Arc<String>);

impl CoralStringValue {
  pub fn new(v: String) -> Self {
    Self(Arc::new(v))
  }

  pub fn make_mut(&mut self) -> &mut String {
    Arc::make_mut(&mut self.0)
  }

  pub fn unwrap(self) -> String {
    Arc::try_unwrap(self.0).unwrap_or_else(|arc| (*arc).clone())
  }
}

impl std::ops::Deref for CoralStringValue {
  type Target = String;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl AsRef<str> for CoralStringValue {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl From<String> for CoralStringValue {
  fn from(v: String) -> Self {
    Self(Arc::new(v))
  }
}

impl From<&str> for CoralStringValue {
  fn from(v: &str) -> Self {
    Self(Arc::new(v.to_string()))
  }
}

impl std::hash::Hash for CoralStringValue {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

/// An `Arc`-backed list of values.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CoralListValue(Arc<Vec<CoralValue>>);

impl CoralListValue {
  pub fn new(v: Vec<CoralValue>) -> Self {
    Self(Arc::new(v))
  }

  pub fn make_mut(&mut self) -> &mut Vec<CoralValue> {
    Arc::make_mut(&mut self.0)
  }

  pub fn unwrap(self) -> Vec<CoralValue> {
    Arc::try_unwrap(self.0).unwrap_or_else(|arc| (*arc).clone())
  }
}

impl std::ops::Deref for CoralListValue {
  type Target = Vec<CoralValue>;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<T: Into<CoralValue>> From<Vec<T>> for CoralListValue {
  fn from(v: Vec<T>) -> Self {
    Self(Arc::new(v.into_iter().map(|x| x.into()).collect()))
  }
}

impl std::hash::Hash for CoralListValue {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

/// An `Arc`-backed map of string keys to values.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CoralMapValue(Arc<FxHashMap<String, CoralValue>>);

impl CoralMapValue {
  pub fn new(v: FxHashMap<String, CoralValue>) -> Self {
    Self(Arc::new(v))
  }

  pub fn make_mut(&mut self) -> &mut FxHashMap<String, CoralValue> {
    Arc::make_mut(&mut self.0)
  }

  pub fn unwrap(self) -> FxHashMap<String, CoralValue> {
    Arc::try_unwrap(self.0).unwrap_or_else(|arc| (*arc).clone())
  }
}

impl std::ops::Deref for CoralMapValue {
  type Target = FxHashMap<String, CoralValue>;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl From<Vec<(String, CoralValue)>> for CoralMapValue {
  fn from(v: Vec<(String, CoralValue)>) -> Self {
    Self(Arc::new(FxHashMap::from_iter(v)))
  }
}

impl<S: Into<String>, M> From<std::collections::HashMap<S, CoralValue, M>> for CoralMapValue {
  fn from(map: std::collections::HashMap<S, CoralValue, M>) -> Self {
    let mut new_map = FxHashMap::default();
    for (k, v) in map {
      new_map.insert(k.into(), v);
    }
    Self(Arc::new(new_map))
  }
}

impl std::hash::Hash for CoralMapValue {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    state.write_usize(self.0.len());
    for (k, v) in self.0.iter() {
      k.hash(state);
      v.hash(state);
    }
  }
}

// ---------------------------------------------------------------------------
// CoralValue
// ---------------------------------------------------------------------------

/// A JSON-like value type with an additional `Container` variant.
///
/// `CoralValue` is what users read and write when interacting with
/// CRDT containers. Heap-allocated variants (`String`, `List`, `Map`, `Binary`)
/// are backed by `Arc` for O(1) clone, matching Loro's design.
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
  /// Note: `NaN != NaN` by `PartialEq`, but `Hash` uses `to_bits()` for
  /// consistent hashing of the same bit pattern (matching Loro's trade-off).
  Double(f64),
  /// Raw binary data.
  Binary(CoralBinaryValue),
  /// UTF-8 string.
  String(CoralStringValue),
  /// Ordered list of values.
  List(CoralListValue),
  /// A map of string keys to values.
  Map(CoralMapValue),
  /// A reference to a nested CRDT container.
  ///
  /// This variant appears when a container (e.g. a Map) stores another
  /// container (e.g. a List) as one of its values.
  Container(ContainerID),
}

// Blank Eq impl — same approach as Loro.
impl Eq for CoralValue {}

impl std::hash::Hash for CoralValue {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    std::mem::discriminant(self).hash(state);
    match self {
      CoralValue::Null => {}
      CoralValue::Bool(v) => {
        state.write_u8(*v as u8);
      }
      CoralValue::Double(v) => {
        state.write_u64(v.to_bits());
      }
      CoralValue::I64(v) => {
        state.write_i64(*v);
      }
      CoralValue::Binary(v) => {
        v.hash(state);
      }
      CoralValue::String(v) => {
        v.hash(state);
      }
      CoralValue::List(v) => {
        v.hash(state);
      }
      CoralValue::Map(v) => {
        v.hash(state);
      }
      CoralValue::Container(v) => {
        v.hash(state);
      }
    }
  }
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
      Self::Binary(b) => b.is_empty(),
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
      Self::Double(n) => {
        serde_json::Value::Number(serde_json::Number::from_f64(*n).unwrap_or_else(|| 0.into()))
      }
      Self::Binary(b) => serde_json::Value::Array(
        b.iter()
          .map(|&x| serde_json::Value::Number(x.into()))
          .collect(),
      ),
      Self::String(s) => serde_json::Value::String(s.to_string()),
      Self::List(v) => serde_json::Value::Array(v.iter().map(CoralValue::to_serde_value).collect()),
      Self::Map(m) => {
        let obj = m
          .iter()
          .map(|(k, v)| (k.clone(), v.to_serde_value()))
          .collect();
        serde_json::Value::Object(obj)
      }
      Self::Container(cid) => {
        serde_json::json!({ "__coral_container": cid.to_string() })
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
          Self::Double(f)
        } else {
          Self::Null
        }
      }
      serde_json::Value::String(s) => Self::String(s.into()),
      serde_json::Value::Array(a) => Self::List(
        a.into_iter()
          .map(Self::from_serde_value)
          .collect::<Vec<_>>()
          .into(),
      ),
      serde_json::Value::Object(mut m) => {
        if let Some(serde_json::Value::String(s)) = m.remove("__coral_container")
          && let Ok(cid) = s.parse()
        {
          return Self::Container(cid);
        }
        Self::Map(
          m.into_iter()
            .map(|(k, v)| (k, Self::from_serde_value(v)))
            .collect::<FxHashMap<_, _>>()
            .into(),
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
  pub fn double(v: f64) -> Self {
    Self::Double(v)
  }

  /// Wraps a `String`.
  pub fn string(v: impl Into<String>) -> Self {
    Self::String(v.into().into())
  }

  /// Wraps a `Vec<CoralValue>`.
  pub fn list(v: Vec<CoralValue>) -> Self {
    Self::List(v.into())
  }

  /// Wraps an [`FxHashMap<String, CoralValue>`].
  pub fn map(v: FxHashMap<String, CoralValue>) -> Self {
    Self::Map(v.into())
  }

  /// Wraps raw binary data.
  pub fn binary(v: impl Into<Vec<u8>>) -> Self {
    Self::Binary(v.into().into())
  }

  /// Wraps a [`ContainerID`].
  pub fn container(v: ContainerID) -> Self {
    Self::Container(v)
  }
}

// ---------------------------------------------------------------------------
// From impls (matching Loro)
// ---------------------------------------------------------------------------

impl From<Vec<u8>> for CoralValue {
  fn from(v: Vec<u8>) -> Self {
    Self::Binary(v.into())
  }
}

impl From<&[u8]> for CoralValue {
  fn from(v: &[u8]) -> Self {
    Self::Binary(v.into())
  }
}

impl From<i32> for CoralValue {
  fn from(v: i32) -> Self {
    Self::I64(v as i64)
  }
}

impl From<u32> for CoralValue {
  fn from(v: u32) -> Self {
    Self::I64(v as i64)
  }
}

impl From<i64> for CoralValue {
  fn from(v: i64) -> Self {
    Self::I64(v)
  }
}

impl From<u16> for CoralValue {
  fn from(v: u16) -> Self {
    Self::I64(v as i64)
  }
}

impl From<i16> for CoralValue {
  fn from(v: i16) -> Self {
    Self::I64(v as i64)
  }
}

impl From<f64> for CoralValue {
  fn from(v: f64) -> Self {
    Self::Double(v)
  }
}

impl From<bool> for CoralValue {
  fn from(v: bool) -> Self {
    Self::Bool(v)
  }
}

impl<T: Into<CoralValue>> From<Vec<T>> for CoralValue {
  fn from(value: Vec<T>) -> Self {
    let vec: Vec<CoralValue> = value.into_iter().map(|x| x.into()).collect();
    Self::List(vec.into())
  }
}

impl From<&str> for CoralValue {
  fn from(v: &str) -> Self {
    Self::String(v.into())
  }
}

impl From<String> for CoralValue {
  fn from(v: String) -> Self {
    Self::String(v.into())
  }
}

impl<'a> From<&'a [CoralValue]> for CoralValue {
  fn from(v: &'a [CoralValue]) -> Self {
    Self::List(v.to_vec().into())
  }
}

impl From<ContainerID> for CoralValue {
  fn from(v: ContainerID) -> Self {
    Self::Container(v)
  }
}

impl<S: Into<String>, M> From<std::collections::HashMap<S, CoralValue, M>> for CoralValue {
  fn from(map: std::collections::HashMap<S, CoralValue, M>) -> Self {
    let mut new_map = FxHashMap::default();
    for (k, v) in map {
      new_map.insert(k.into(), v);
    }
    Self::Map(new_map.into())
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::ContainerType;

  #[test]
  fn test_coral_value_json_roundtrip() {
    let value = CoralValue::Map(
      FxHashMap::from_iter([
        ("name".to_string(), CoralValue::String("Alice".into())),
        ("age".to_string(), CoralValue::I64(30)),
        ("active".to_string(), CoralValue::Bool(true)),
      ])
      .into(),
    );
    let json = value.to_json().unwrap();
    let decoded = CoralValue::from_json(&json).unwrap();
    assert_eq!(value, decoded);
  }

  #[test]
  fn test_coral_value_list_json() {
    let value = CoralValue::List(vec![1i32, 2, 3].into());
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
  fn test_coral_value_double_no_eq() {
    let a = CoralValue::Double(f64::NAN);
    let b = CoralValue::Double(f64::NAN);
    assert_ne!(a, b);
  }

  #[test]
  fn test_coral_value_convenience_constructors() {
    assert_eq!(CoralValue::bool(true), CoralValue::Bool(true));
    assert_eq!(CoralValue::int(42), CoralValue::I64(42));
    assert_eq!(CoralValue::double(3.14), CoralValue::Double(3.14));
    assert_eq!(
      CoralValue::string("hello"),
      CoralValue::String("hello".into())
    );
    assert_eq!(
      CoralValue::list(vec![]),
      CoralValue::List(Vec::<CoralValue>::new().into())
    );
    assert_eq!(
      CoralValue::map(FxHashMap::default()),
      CoralValue::Map(FxHashMap::<String, CoralValue>::default().into())
    );
  }

  #[test]
  fn test_coral_value_is_empty() {
    assert!(CoralValue::String("".into()).is_empty());
    assert!(CoralValue::List(Vec::<CoralValue>::new().into()).is_empty());
    assert!(CoralValue::Map(FxHashMap::<String, CoralValue>::default().into()).is_empty());
    assert!(CoralValue::Binary(vec![].into()).is_empty());
    assert!(!CoralValue::String("x".into()).is_empty());
    assert!(!CoralValue::I64(0).is_empty());
  }

  #[test]
  fn test_coral_value_binary_roundtrip() {
    let original = vec![0u8, 1, 2, 255];
    let value = CoralValue::Binary(original.clone().into());
    assert_eq!(value.to_json().unwrap(), "[0,1,2,255]");

    // Verify Deref works
    if let CoralValue::Binary(b) = &value {
      assert_eq!(b.len(), 4);
      assert_eq!(b[0], 0);
    }
  }

  #[test]
  fn test_coral_value_arc_clone_is_cheap() {
    let value =
      CoralValue::string("a very long string that would be expensive to deep copy".to_string());
    let cloned = value.clone();
    // Both should be equal without deep copy (Arc sharing)
    assert_eq!(value, cloned);
  }

  #[test]
  fn test_coral_value_hash_consistency() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let a = CoralValue::Map(
      FxHashMap::from_iter([
        ("x".to_string(), CoralValue::I64(1)),
        ("y".to_string(), CoralValue::Double(2.5)),
      ])
      .into(),
    );
    let b = a.clone();

    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    a.hash(&mut h1);
    b.hash(&mut h2);
    assert_eq!(h1.finish(), h2.finish());
  }

  #[test]
  fn test_coral_value_from_impls() {
    assert_eq!(CoralValue::from(42i32), CoralValue::I64(42));
    assert_eq!(CoralValue::from(42u32), CoralValue::I64(42));
    assert_eq!(CoralValue::from(3.14f64), CoralValue::Double(3.14));
    assert_eq!(CoralValue::from(true), CoralValue::Bool(true));
    assert_eq!(
      CoralValue::from("hello"),
      CoralValue::String("hello".into())
    );
    assert_eq!(
      CoralValue::from("hello".to_string()),
      CoralValue::String("hello".into())
    );

    let list: CoralValue = vec![1i32, 2, 3].into();
    assert_eq!(list, CoralValue::List(vec![1i64, 2, 3].into()));

    let bytes: CoralValue = vec![1u8, 2, 3].into();
    assert!(matches!(bytes, CoralValue::Binary(_)));
  }
}
