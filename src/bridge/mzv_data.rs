//! MZV data-source resolution for the JSON compatibility bridge.

use std::ffi::OsString;
use std::path::PathBuf;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::reduce::{MzvReductionTable, load_mzv_reductions, standard_mzv_reductions};

#[derive(Clone, Debug, PartialEq, Eq)]
enum MzvDataSource {
    Path(PathBuf),
    Embedded,
}

fn mzv_data_source(request: &Value, data_dir: Option<OsString>) -> Result<MzvDataSource> {
    if let Some(value) = request.get("mzv_data_path") {
        let path = value.as_str().ok_or_else(|| {
            Error::InvalidInput("`mzv_data_path` must be a filesystem path string".into())
        })?;
        return Ok(MzvDataSource::Path(PathBuf::from(path)));
    }
    if let Some(data_dir) = data_dir.filter(|value| !value.is_empty()) {
        return Ok(MzvDataSource::Path(
            PathBuf::from(data_dir).join("mzv_reductions.json"),
        ));
    }
    Ok(MzvDataSource::Embedded)
}

/// Resolve bridge data without depending on the build checkout.
///
/// Compatibility callers may override the embedded table with an explicit
/// request path, followed by `HYPERFLINT_DATA_DIR/mzv_reductions.json`.
pub(super) fn mzv_reduction_table(request: &Value) -> Result<MzvReductionTable> {
    match mzv_data_source(request, std::env::var_os("HYPERFLINT_DATA_DIR"))? {
        MzvDataSource::Path(path) => load_mzv_reductions(path),
        MzvDataSource::Embedded => Ok(standard_mzv_reductions()),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use serde_json::json;

    use super::*;

    #[test]
    fn source_priority_is_request_then_environment_then_embedded() {
        let explicit = json!({"mzv_data_path": "/request/table.json"});
        assert_eq!(
            mzv_data_source(&explicit, Some(OsString::from("/environment"))).unwrap(),
            MzvDataSource::Path(PathBuf::from("/request/table.json"))
        );
        assert_eq!(
            mzv_data_source(&json!({}), Some(OsString::from("/environment"))).unwrap(),
            MzvDataSource::Path(PathBuf::from("/environment/mzv_reductions.json"))
        );
        assert_eq!(
            mzv_data_source(&json!({}), None).unwrap(),
            MzvDataSource::Embedded
        );
        assert!(mzv_data_source(&json!({"mzv_data_path": 7}), None).is_err());
    }
}
