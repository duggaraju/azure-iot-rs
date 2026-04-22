mod config;
mod deviceclient;
mod error;
mod hub;
mod message;
mod moduleclient;
mod storage;
mod structs;
mod transport;

pub use azure_iot_rs_sys::enums::*;

pub type IoTHubMessageResult = IothubMessageResult;
pub type IoTHubMessageContentType = IothubmessageContentType;
pub type IoTHubClientFileUploadResult = IothubClientFileUploadResult;
pub type IoTHubClientResult = IothubClientResult;
pub type IoTHubClientRetryPolicy = IothubClientRetryPolicy;
pub type IoTHubClientStatus = IothubClientStatus;
pub type IoTHubIdentityType = IothubIdentityType;
pub type IoTHubProcessItemResult = IothubProcessItemResult;
pub type IoTHubMessageDispositionResult = IothubmessageDispositionResult;
pub type IoTHubClientIoTHubMethodStatus = IothubClientIothubMethodStatus;
pub type IoTHubClientConfirmationResult = IothubClientConfirmationResult;
pub type ConnectionStatus = IothubClientConnectionStatus;
pub type ConnectionStatusReason = IothubClientConnectionStatusReason;
pub type IoTHubClientPropertyPayloadType = IothubClientPropertyPayloadType;
pub type IoTHubDeviceTwinUpdateState = DeviceTwinUpdateState;
pub type IotHubDeviceTwinUpdateState = DeviceTwinUpdateState;
pub type IoTHubClientFileUploadGetDataResult = IothubClientFileUploadGetDataResult;

pub use config::ConfigBuilder;
pub use deviceclient::IoTHubDeviceClient;
pub use error::IotError;
pub use hub::IotHub;
pub use message::{IotHubMessage, MessageBody};
pub use moduleclient::{IotHubModuleClient, ModuleClientOption, ModuleEventCallback};
pub use structs::{IoTHubClientCommandRequest, IoTHubClientCommandResponse};
pub use transport::Transport;

#[cfg(test)]
mod tests {

    use super::*;

    struct TestModuleEventCallback;
    impl ModuleEventCallback for TestModuleEventCallback {
        // Implement the required methods for the test callback
    }

    #[test]
    fn initialize_client() {
        let callback = TestModuleEventCallback {};
        let mut client = IotHubModuleClient::try_new(callback).unwrap();
        client
            .set_option(ModuleClientOption::LogTrace(true))
            .unwrap();
        client.do_work();
    }
}
