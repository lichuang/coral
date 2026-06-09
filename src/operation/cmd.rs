use crate::encoding::json::JsonCmd;
use crate::rle::{HasLength, Mergeable};

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
  IncCounter { delta: f64 },
}

impl Cmd {
  pub fn to_json_cmd(&self) -> JsonCmd {
    match self {
      Self::IncCounter { delta } => JsonCmd::IncCounter { delta: *delta },
    }
  }

  pub fn from_json_cmd(jc: JsonCmd) -> Self {
    match jc {
      JsonCmd::IncCounter { delta } => Self::IncCounter { delta },
    }
  }
}

impl HasLength for Cmd {
  fn content_len(&self) -> usize {
    1
  }
}

impl Mergeable for Cmd {
  fn is_mergeable(&self, _other: &Self) -> bool {
    false
  }

  fn merge(&mut self, _other: &Self) {
    unreachable!()
  }
}
