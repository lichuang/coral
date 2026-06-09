use crate::common::CoralResult;
use crate::core::DocInner;
use crate::version::VersionVector;

mod json;

pub use json::JsonSchema;

pub fn export_json(
  doc: &DocInner,
  start_vv: &VersionVector,
  end_vv: &VersionVector,
) -> CoralResult<String> {
  let schema = json::build_schema(doc, start_vv, end_vv)?;
  let json = serde_json::to_string_pretty(&schema)?;
  Ok(json)
}
