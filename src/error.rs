use nanoserde::{DeBin, SerBin};
use thiserror::Error;

#[derive(Error, Debug, DeBin, SerBin)]
#[error("Remote error: {}", msg)]
pub struct ExecutionError {
    msg: String,
}

impl From<wasmer::RuntimeError> for ExecutionError {
    fn from(e: wasmer::RuntimeError) -> Self {
        Self { msg: e.to_string() }
    }
}

impl From<wasmer::ExportError> for ExecutionError {
    fn from(e: wasmer::ExportError) -> Self {
        Self { msg: e.to_string() }
    }
}
