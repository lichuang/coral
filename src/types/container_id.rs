//! Unique identifier for a container instance.
//!
//! A [`ContainerID`] identifies a specific CRDT container within a document.
//! There are two flavors:
//!
//! - **Root** containers are top-level containers explicitly created by the
//!   user (e.g. a map named `"settings"`).
//! - **Normal** containers are created automatically as children of other
//!   containers. They are identified by the [`ID`](super::ID) of the operation
//!   that created them.

use super::{ContainerType, Counter, ID, PeerID};
use std::fmt;

/// Unique identifier for a container.
///
/// See the module-level documentation for the distinction between `Root`
/// and `Normal` containers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainerID {
  /// A user-created top-level container.
  Root {
    name: String,
    container_type: ContainerType,
  },
  /// A container created as a child of another container.
  Normal {
    peer: PeerID,
    counter: Counter,
    container_type: ContainerType,
  },
}

impl ContainerID {
  /// Creates a new root container ID.
  ///
  /// # Panics
  ///
  /// Panics if `name` is empty or contains a `'/'` or `'\0'` character.
  pub fn new_root(name: &str, container_type: ContainerType) -> Self {
    assert!(
      !name.is_empty() && !name.contains('/') && !name.contains('\0'),
      "invalid root container name: must be non-empty and not contain '/' or '\\0'"
    );
    Self::Root {
      name: name.to_owned(),
      container_type,
    }
  }

  /// Creates a new normal container ID from an operation [`ID`] and type.
  pub const fn new_normal(id: ID, container_type: ContainerType) -> Self {
    Self::Normal {
      peer: id.peer,
      counter: id.counter,
      container_type,
    }
  }

  /// Returns the [`ContainerType`] of this container.
  pub const fn container_type(&self) -> ContainerType {
    match self {
      Self::Root { container_type, .. } => *container_type,
      Self::Normal { container_type, .. } => *container_type,
    }
  }

  // ═══════════════════════════════════════════════════════════════════════
  // Binary encoding
  // ═══════════════════════════════════════════════════════════════════════

  /// Encodes the `ContainerID` into a compact byte representation.
  ///
  /// Format:
  /// - Root: `[flag | type, name_len (u8), name_bytes...]`
  ///   where `flag = 0x80 | type_u8`
  /// - Normal: `[type (u8), peer (8 bytes LE), counter (4 bytes LE)]`
  pub fn to_bytes(&self) -> Vec<u8> {
    match self {
      Self::Root {
        name,
        container_type,
      } => {
        let mut buf = Vec::with_capacity(2 + name.len());
        buf.push(0x80 | container_type.to_u8());
        buf.push(name.len() as u8);
        buf.extend_from_slice(name.as_bytes());
        buf
      }
      Self::Normal {
        peer,
        counter,
        container_type,
      } => {
        let mut buf = Vec::with_capacity(13);
        buf.push(container_type.to_u8());
        buf.extend_from_slice(&peer.to_le_bytes());
        buf.extend_from_slice(&counter.to_le_bytes());
        buf
      }
    }
  }

  /// Decodes a `ContainerID` from bytes produced by [`to_bytes`](Self::to_bytes).
  ///
  /// Returns `None` if the data is malformed.
  pub fn from_bytes(data: &[u8]) -> Option<Self> {
    if data.is_empty() {
      return None;
    }
    let first = data[0];
    let is_root = (first & 0x80) != 0;
    let type_byte = first & 0x7F;
    let container_type = ContainerType::try_from_u8(type_byte)?;

    if is_root {
      if data.len() < 2 {
        return None;
      }
      let name_len = data[1] as usize;
      if data.len() != 2 + name_len {
        return None;
      }
      let name = String::from_utf8(data[2..].to_vec()).ok()?;
      Some(Self::Root {
        name,
        container_type,
      })
    } else {
      if data.len() != 13 {
        return None;
      }
      let peer = PeerID::from_le_bytes(data[1..9].try_into().ok()?);
      let counter = Counter::from_le_bytes(data[9..13].try_into().ok()?);
      Some(Self::Normal {
        peer,
        counter,
        container_type,
      })
    }
  }
}

impl fmt::Display for ContainerID {
  /// Human-readable representation.
  ///
  /// - Root: `root:<name>:<type>`
  /// - Normal: `<counter>@<peer>:<type>`
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Root {
        name,
        container_type,
      } => write!(f, "root:{name}:{container_type}"),
      Self::Normal {
        peer,
        counter,
        container_type,
      } => write!(f, "{counter}@{peer}:{container_type}"),
    }
  }
}

impl std::str::FromStr for ContainerID {
  type Err = String;

  /// Parses a `ContainerID` from its string representation.
  ///
  /// Expected formats match those produced by [`Display`](ContainerID::Display).
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if let Some(rest) = s.strip_prefix("root:") {
      // root:<name>:<type>
      let colon_pos = rest
        .rfind(':')
        .ok_or_else(|| "missing ':' separator before type".to_string())?;
      let name = &rest[..colon_pos];
      let type_str = &rest[colon_pos + 1..];
      if name.is_empty() {
        return Err("root container name must not be empty".to_string());
      }
      let container_type = type_str
        .parse()
        .map_err(|_| format!("unknown container type: {type_str}"))?;
      Ok(Self::new_root(name, container_type))
    } else {
      // <counter>@<peer>:<type>
      let at_pos = s
        .find('@')
        .ok_or_else(|| "missing '@' separator".to_string())?;
      let colon_pos = s
        .rfind(':')
        .ok_or_else(|| "missing ':' separator before type".to_string())?;
      if colon_pos <= at_pos {
        return Err("invalid format: ':' must come after '@'".to_string());
      }
      let counter: Counter = s[..at_pos]
        .parse()
        .map_err(|_| format!("invalid counter: {}", &s[..at_pos]))?;
      let peer: PeerID = s[at_pos + 1..colon_pos]
        .parse()
        .map_err(|_| format!("invalid peer: {}", &s[at_pos + 1..colon_pos]))?;
      let type_str = &s[colon_pos + 1..];
      let container_type = type_str
        .parse()
        .map_err(|_| format!("unknown container type: {type_str}"))?;
      Ok(Self::new_normal(ID::new(peer, counter), container_type))
    }
  }
}

/// Parse [`ContainerType`] from its string name (used by `FromStr`).
impl std::str::FromStr for ContainerType {
  type Err = ();

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Map" | "map" => Ok(ContainerType::Map),
      "List" | "list" => Ok(ContainerType::List),
      "Text" | "text" => Ok(ContainerType::Text),
      "MovableList" | "movablelist" | "movable_list" => Ok(ContainerType::MovableList),
      "Tree" | "tree" => Ok(ContainerType::Tree),
      "Counter" | "counter" => Ok(ContainerType::Counter),
      _ => Err(()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_container_id_new_root() {
    let id = ContainerID::new_root("my_map", ContainerType::Map);
    assert_eq!(id.container_type(), ContainerType::Map);
  }

  #[test]
  #[should_panic]
  fn test_container_id_new_root_empty_name() {
    ContainerID::new_root("", ContainerType::Map);
  }

  #[test]
  #[should_panic]
  fn test_container_id_new_root_invalid_char() {
    ContainerID::new_root("a/b", ContainerType::Map);
  }

  #[test]
  fn test_container_id_new_normal() {
    let id = ContainerID::new_normal(ID::new(42, 7), ContainerType::List);
    assert_eq!(id.container_type(), ContainerType::List);
  }

  #[test]
  fn test_container_id_bytes_roundtrip_root() {
    let original = ContainerID::new_root("settings", ContainerType::Map);
    let bytes = original.to_bytes();
    let decoded = ContainerID::from_bytes(&bytes).unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn test_container_id_bytes_roundtrip_normal() {
    let original = ContainerID::new_normal(ID::new(0xDEADBEEF, 12345), ContainerType::Text);
    let bytes = original.to_bytes();
    let decoded = ContainerID::from_bytes(&bytes).unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn test_container_id_bytes_invalid() {
    assert!(ContainerID::from_bytes(&[]).is_none());
    assert!(ContainerID::from_bytes(&[0xFF]).is_none()); // unknown type
    assert!(ContainerID::from_bytes(&[0x00; 12]).is_none()); // wrong len for Normal
  }

  #[test]
  fn test_container_id_string_roundtrip_root() {
    let original = ContainerID::new_root("my-text", ContainerType::Text);
    let s = original.to_string();
    let decoded: ContainerID = s.parse().unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn test_container_id_string_roundtrip_normal() {
    let original = ContainerID::new_normal(ID::new(42, 7), ContainerType::Counter);
    let s = original.to_string();
    let decoded: ContainerID = s.parse().unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn test_container_id_string_colon_in_name() {
    // Root name containing ':'
    let original = ContainerID::new_root("a:b:c", ContainerType::Map);
    let s = original.to_string();
    let decoded: ContainerID = s.parse().unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn test_container_id_string_invalid() {
    assert!("foo".parse::<ContainerID>().is_err());
    assert!("root::Map".parse::<ContainerID>().is_err()); // empty name
    assert!("7@abc:Map".parse::<ContainerID>().is_err()); // bad peer
  }
}
