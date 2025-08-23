use crate::message::IotHubMessage;
use azure_iot_rs_sys::*;
use log::{error, info};
use serde_json::Value;
use std::boxed::Box;
use std::convert::TryFrom;
use std::ffi::{CString, c_void};
use std::ops::FnMut;
use std::panic;
use std::result::Result;
use std::str;
use std::sync::Once;
use std::{thread, time};

static IOTHUB: Once = Once::new();

/// Enum to describe the type of module event.
#[derive(Debug)]
pub enum IotHubModuleEvent {
    Message(IotHubMessage),
    Twin(Value),
}

pub struct IotHubModuleClient<'c> {
    handle: IOTHUB_MODULE_CLIENT_LL_HANDLE,
    callback: Box<dyn FnMut(IotHubModuleEvent) + 'c>,
}

unsafe impl<'c> Send for IotHubModuleClient<'c> {}

impl<'c> IotHubModuleClient<'c> {
    unsafe extern "C" fn c_message_callback(
        handle: *mut IOTHUB_MESSAGE_HANDLE_DATA_TAG,
        ctx: *mut std::ffi::c_void,
    ) -> IOTHUBMESSAGE_DISPOSITION_RESULT {
        info!("Received message from hub!");
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient) };
        let message = IotHubMessage::from_handle(handle);
        let result = client.message_callback(message);
        match result {
            Result::Ok(_) => IOTHUBMESSAGE_DISPOSITION_RESULT_TAG_IOTHUBMESSAGE_ACCEPTED,
            Result::Err(_) => IOTHUBMESSAGE_DISPOSITION_RESULT_TAG_IOTHUBMESSAGE_REJECTED,
        }
    }

    unsafe extern "C" fn c_twin_callback(
        state: DEVICE_TWIN_UPDATE_STATE,
        payload: *const u8,
        size: usize,
        ctx: *mut std::ffi::c_void,
    ) {
        info!("Received twin callback from hub! {} {}", state, size);
        unsafe {
            let client = &mut *(ctx as *mut IotHubModuleClient);
            let data = std::slice::from_raw_parts(payload, usize::try_from(size).unwrap());
            client.twin_callback(data);
        }
    }

    unsafe extern "C" fn c_confirmation_callback(
        _status: IOTHUB_CLIENT_RESULT,
        ctx: *mut std::ffi::c_void,
    ) {
        let _message = unsafe { &mut *(ctx as *mut Box<IotHubMessage>) };
    }

    fn message_callback(&mut self, message: IotHubMessage) -> Result<(), &str> {
        (self.callback)(IotHubModuleEvent::Message(message));
        Ok(())
    }

    fn twin_callback(&mut self, data: &[u8]) {
        let value = str::from_utf8(data).unwrap();
        let settings: Value = serde_json::from_slice(data).unwrap();
        info!("Received settings {} {}", settings, value);
        (self.callback)(IotHubModuleEvent::Twin(settings));
    }

    pub fn send_message(&self, mut message: Box<IotHubMessage>) -> Result<(), &str> {
        let output = CString::new("output").unwrap();
        unsafe {
            let context = message.as_mut() as *mut IotHubMessage as *mut c_void;
            if IoTHubModuleClient_LL_SendEventToOutputAsync(
                self.handle,
                message.handle,
                output.as_ptr(),
                Some(IotHubModuleClient::c_confirmation_callback),
                context,
            ) == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK
            {
                error!("Failed to send message to the hub!!");
                return Err("Failed to send message");
            }
            return Ok(());
        };
    }

    pub fn new(callback: impl FnMut(IotHubModuleEvent) + 'c) -> Box<Self> {
        unsafe {
            IOTHUB.call_once(|| {
                IoTHub_Init();
            });
            let handle = IoTHubModuleClient_LL_CreateFromEnvironment(Some(MQTT_Protocol));
            if handle.is_null() {
                panic!("Failed to initialize the client from environment!");
            }

            let mut client = Box::new(IotHubModuleClient {
                handle,
                callback: Box::new(callback),
            });
            let context = client.as_mut() as *mut IotHubModuleClient as *mut c_void;
            let input = CString::new("input").unwrap();
            if IoTHubModuleClient_LL_SetInputMessageCallback(
                handle,
                input.as_ptr(),
                Some(IotHubModuleClient::c_message_callback),
                context,
            ) != IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK
            {
                panic!("Failed to set the message callback");
            }

            if IoTHubModuleClient_LL_SetModuleTwinCallback(
                handle,
                Some(IotHubModuleClient::c_twin_callback),
                context,
            ) != IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK
            {
                panic!("Failed to set twin callback!");
            }
            return client;
        }
    }

    pub fn do_work(&mut self) {
        loop {
            unsafe {
                IoTHubModuleClient_LL_DoWork(self.handle);
            }
            let hundred_millis = time::Duration::from_millis(100);
            thread::sleep(hundred_millis);
        }
    }
}

impl<'c> Drop for IotHubModuleClient<'c> {
    fn drop(&mut self) {
        unsafe {
            IoTHubModuleClient_LL_Destroy(self.handle);
        }
    }
}
