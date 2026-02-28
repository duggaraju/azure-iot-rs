use azure_iot_rs_sys::{IOTHUB_CLIENT_RESULT, IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IotError {
    #[error("SDK error: {0}")]
    Sdk(IOTHUB_CLIENT_RESULT),
    #[error("Null pointer returned from SDK")]
    NullPtr,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CString error: {0}")]
    Null(#[from] std::ffi::NulError),

    #[error("HTTP error: {0}")]
    Http(u16),
}

impl IotError {
    pub fn check_sdk_result(result: IOTHUB_CLIENT_RESULT) -> Result<(), IotError> {
        if result == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK {
            Ok(())
        } else {
            Err(Self::Sdk(result))
        }
    }
}
