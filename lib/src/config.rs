use azure_iot_rs_sys::{IOTHUB_CLIENT_CONFIG, IOTHUB_CLIENT_TRANSPORT_PROVIDER};
use std::ffi::CString;

use crate::error::IotError;

pub struct ConfigBuilder {
    protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
    device_id: CString,
    device_key: Option<CString>,
    device_sas_token: Option<CString>,
    iot_hub_name: CString,
    iot_hub_suffix: CString,
    protocol_gateway_host_name: Option<CString>,
}

impl ConfigBuilder {
    pub fn new(
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
        device_id: &str,
        iot_hub_name: &str,
        iot_hub_suffix: &str,
    ) -> Result<Self, IotError> {
        let device_id = CString::new(device_id)?;
        let iot_hub_name = CString::new(iot_hub_name)?;
        let iot_hub_suffix = CString::new(iot_hub_suffix)?;

        Ok(Self {
            protocol,
            device_id,
            device_key: None,
            device_sas_token: None,
            iot_hub_name,
            iot_hub_suffix,
            protocol_gateway_host_name: None,
        })
    }

    pub fn with_device_key(mut self, device_key: &str) -> Result<Self, IotError> {
        self.device_key = Some(CString::new(device_key)?);
        Ok(self)
    }

    pub fn with_device_sas_token(mut self, device_sas_token: &str) -> Result<Self, IotError> {
        self.device_sas_token = Some(CString::new(device_sas_token)?);
        Ok(self)
    }

    pub fn with_protocol_gateway_host_name(
        mut self,
        protocol_gateway_host_name: &str,
    ) -> Result<Self, IotError> {
        self.protocol_gateway_host_name = Some(CString::new(protocol_gateway_host_name)?);
        Ok(self)
    }

    pub fn build(&self) -> IOTHUB_CLIENT_CONFIG {
        IOTHUB_CLIENT_CONFIG {
            protocol: self.protocol,
            deviceId: self.device_id.as_ptr(),
            deviceKey: self
                .device_key
                .as_ref()
                .map_or(std::ptr::null(), |v| v.as_ptr()),
            deviceSasToken: self
                .device_sas_token
                .as_ref()
                .map_or(std::ptr::null(), |v| v.as_ptr()),
            iotHubName: self.iot_hub_name.as_ptr(),
            iotHubSuffix: self.iot_hub_suffix.as_ptr(),
            protocolGatewayHostName: self
                .protocol_gateway_host_name
                .as_ref()
                .map_or(std::ptr::null(), |v| v.as_ptr()),
        }
    }
}
