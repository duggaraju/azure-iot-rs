use azure_iot_rs_sys::*;
use std::ffi::CStr;

use crate::IoTHubMessageContentType;

/// Enum to return the type of an IOT hub message.
#[derive(Debug)]
pub enum MessageBody<'b> {
    Text(&'b str),
    Binary(&'b [u8]),
}

/// A struct to represet an IOT hub message.
#[derive(Debug)]
pub struct IotHubMessage {
    pub(crate) handle: IOTHUB_MESSAGE_HANDLE,
    own: bool,
}

impl Drop for IotHubMessage {
    fn drop(&mut self) {
        if self.own {
            unsafe {
                IoTHubMessage_Destroy(self.handle);
            }
        }
    }
}

impl Clone for IotHubMessage {
    fn clone(&self) -> Self {
        let handle = unsafe { IoTHubMessage_Clone(self.handle) };
        if handle.is_null() {
            panic!("Failed to allocate message");
        }
        IotHubMessage { handle, own: true }
    }
}

impl IotHubMessage {
    pub fn from(handle: IOTHUB_MESSAGE_HANDLE) -> Self {
        IotHubMessage { handle, own: true }
    }

    fn to_bytes(&self) -> &[u8] {
        let buffer: *mut *const ::std::os::raw::c_uchar = std::ptr::null_mut();
        let size: *mut usize = std::ptr::null_mut();
        unsafe {
            IoTHubMessage_GetByteArray(self.handle, buffer, size);
            std::slice::from_raw_parts(*buffer, *size as usize)
        }
    }

    fn to_str(&self) -> &str {
        let ptr = unsafe { IoTHubMessage_GetString(self.handle) };
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr).to_str().unwrap_or_default() }
    }

    pub fn content_type(&self) -> IoTHubMessageContentType {
        unsafe { IoTHubMessage_GetContentType(self.handle).into() }
    }

    pub fn from_handle(handle: IOTHUB_MESSAGE_HANDLE) -> Self {
        IotHubMessage { handle, own: false }
    }

    pub fn body<'a>(&'a self) -> MessageBody<'a> {
        let content_type = self.content_type();
        match content_type {
            IoTHubMessageContentType::String => MessageBody::Text(self.to_str()),
            IoTHubMessageContentType::Bytearray => MessageBody::Binary(self.to_bytes()),
            _ => panic!("Unknown content type"),
        }
    }
}
