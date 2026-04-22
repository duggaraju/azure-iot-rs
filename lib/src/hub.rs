use azure_iot_rs_sys::{IOTHUB_CLIENT_RESULT, IoTHub_Deinit, IoTHub_Init};
use std::{os::raw::c_int, sync::OnceLock};

use crate::IotError;

static IOTHUB: OnceLock<IotHub> = OnceLock::new();

pub struct IotHub(c_int);

impl IotHub {
    pub fn ensure_initialized() -> Result<(), IotError> {
        let hub = IOTHUB.get_or_init(|| {
            let result = unsafe { IoTHub_Init() };
            Self(result)
        });
        IotError::check_sdk_result(hub.0 as IOTHUB_CLIENT_RESULT)
    }
}

impl Drop for IotHub {
    fn drop(&mut self) {
        if self.0 == 0 {
            unsafe {
                IoTHub_Deinit();
            }
        }
    }
}
