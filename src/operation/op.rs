use crate::common::{CoralError, CoralResult};
use crate::encoding::json::JsonOp;
use crate::object::ObjectRegistry;
use crate::operation::Cmd;
use crate::rle::{HasLength, Mergeable};
use crate::types::{Counter, CounterExt, ObjectId, ObjectIndex};

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
  /// Absolute counter value within the peer's sequence.
  ///
  /// This is the full counter (not a relative offset) and matches the
  /// corresponding position in the enclosing [`Commit`](crate::core::Commit).
  pub counter: Counter,
  pub container: ObjectIndex,
  pub cmd: Cmd,
}

impl Op {
  pub const fn new(counter: Counter, container: ObjectIndex, cmd: Cmd) -> Self {
    Self {
      counter,
      container,
      cmd,
    }
  }

  pub fn to_json_op(&self, registry: &ObjectRegistry) -> CoralResult<JsonOp> {
    let container = registry
      .get_by_index(self.container)
      .cloned()
      .ok_or_else(|| {
        CoralError::InvalidExport(format!(
          "unknown container index {}",
          self.container.index()
        ))
      })?;
    Ok(JsonOp {
      container,
      cmd: self.cmd.to_json_cmd(),
    })
  }

  pub fn from_json_op(
    jop: JsonOp,
    ensure_container: &mut dyn FnMut(&str, ObjectId) -> CoralResult<ObjectIndex>,
  ) -> CoralResult<Self> {
    let cmd = Cmd::from_json_cmd(jop.cmd);
    let name = match &jop.container {
      ObjectId::Root { name, .. } => name.clone(),
      _ => return Err(CoralError::InvalidImport("node container".into())),
    };
    let container = ensure_container(&name, jop.container)?;
    Ok(Self::new(0, container, cmd))
  }
}

impl HasLength for Op {
  fn content_len(&self) -> usize {
    1
  }
}

impl Mergeable for Op {
  fn is_mergeable(&self, other: &Self) -> bool {
    self
      .counter
      .is_consecutive(other.counter, self.content_len() as Counter)
      && self.container == other.container
      && self.cmd.is_mergeable(&other.cmd)
  }

  fn merge(&mut self, other: &Self) {
    self.cmd.merge(&other.cmd);
  }
}
