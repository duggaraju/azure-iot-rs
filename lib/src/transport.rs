use azure_iot_rs_sys::{
    IOTHUB_CLIENT_TRANSPORT_PROVIDER, IoTHubTransport_Create, IoTHubTransport_Destroy,
    MQTT_Protocol, TRANSPORT_HANDLE,
};
use std::ffi::CStr;

use crate::error::IotError;

pub enum TransportProvider {
    Http,
    Amqp,
    Mqtt,
}

impl TransportProvider {
    pub(crate) unsafe fn to_sdk(&self) -> IOTHUB_CLIENT_TRANSPORT_PROVIDER {
        match self {
            TransportProvider::Http => None,
            TransportProvider::Amqp => None,
            TransportProvider::Mqtt => Some(MQTT_Protocol),
        }
    }
}

pub struct Transport(TRANSPORT_HANDLE);

impl Transport {
    pub fn from(
        protocol: TransportProvider,
        iot_hub_name: &CStr,
        iot_hub_suffix: &CStr,
    ) -> Result<Self, IotError> {
        let protocol = unsafe { protocol.to_sdk() };
        let handle = unsafe {
            IoTHubTransport_Create(protocol, iot_hub_name.as_ptr(), iot_hub_suffix.as_ptr())
        };
        if handle.is_null() {
            Err(IotError::Sdk(0)) // Replace with actual error code if available
        } else {
            Ok(Self(handle))
        }
    }

    pub fn from_raw(raw: TRANSPORT_HANDLE) -> Option<Self> {
        if raw.is_null() { None } else { Some(Self(raw)) }
    }

    pub(crate) fn as_raw(&self) -> TRANSPORT_HANDLE {
        self.0
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        unsafe {
            IoTHubTransport_Destroy(self.0);
        }
    }
}
