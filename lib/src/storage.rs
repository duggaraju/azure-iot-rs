use azure_iot_rs_sys::*;

use crate::error::IotError;

pub struct UploadContextHandle(IOTHUB_CLIENT_LL_UPLOADTOBLOB_CONTEXT_HANDLE);

impl UploadContextHandle {
    pub fn from_raw(raw: IOTHUB_CLIENT_LL_UPLOADTOBLOB_CONTEXT_HANDLE) -> Option<Self> {
        if raw.is_null() { None } else { Some(Self(raw)) }
    }

    pub fn put_block(&self, block_number: u32, data: &[u8]) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_AzureStoragePutBlock(self.0, block_number, data.as_ptr(), data.len())
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn put_block_list(&self) -> Result<(), IotError> {
        let result = unsafe { IoTHubClientCore_AzureStoragePutBlockList(self.0) };
        IotError::check_sdk_result(result)?;
        Ok(())
    }
}

impl Drop for UploadContextHandle {
    fn drop(&mut self) {
        unsafe { IoTHubClientCore_AzureStorageDestroyClient(self.0) }
    }
}
