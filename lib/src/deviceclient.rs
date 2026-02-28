use crate::config::ConfigBuilder;
use crate::error::IotError;
use crate::message::IotHubMessage;
use crate::storage::UploadContextHandle;
use crate::transport::Transport;
use crate::{
    ConnectionStatus, ConnectionStatusReason, IoTHubClientConfirmationResult,
    IoTHubClientRetryPolicy, IoTHubClientStatus, IoTHubDeviceTwinUpdateState,
    IoTHubMessageDispositionResult,
};
use azure_iot_rs_sys::*;
use futures::channel::oneshot;
use std::ffi::{CStr, c_char, c_void};
use std::ptr::{self, NonNull};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static IOTHUB: Once = Once::new();

pub struct MethodHandle(METHOD_HANDLE);

impl MethodHandle {
    pub fn from_raw(raw: METHOD_HANDLE) -> Option<Self> {
        if raw.is_null() { None } else { Some(Self(raw)) }
    }

    fn as_raw(&self) -> METHOD_HANDLE {
        self.0
    }
}

pub trait DeviceClientCallback {
    fn on_message(&mut self, msg: IotHubMessage) -> IoTHubMessageDispositionResult;
    fn on_input_message(&mut self, msg: IotHubMessage) -> IoTHubMessageDispositionResult;
    fn on_connection_status_changed(
        &mut self,
        status: ConnectionStatus,
        reason: ConnectionStatusReason,
    );
    fn on_device_twin(&mut self, state: IoTHubDeviceTwinUpdateState, data: &[u8]);
    fn on_device_method(&mut self, method_name: &str, payload: &[u8]) -> Result<Vec<u8>, IotError>;
}

#[derive(Clone)]
pub struct IoTHubDeviceClient<C: DeviceClientCallback> {
    handle: IOTHUB_CLIENT_CORE_HANDLE,
    callback: C,
}

// C- callbacks.
impl<C> IoTHubDeviceClient<C>
where
    C: DeviceClientCallback,
{
    fn context_ptr(&mut self) -> *mut c_void {
        self as *mut Self as *mut c_void
    }

    unsafe extern "C" fn c_message_callback(
        message: IOTHUB_MESSAGE_HANDLE,
        ctx: *mut c_void,
    ) -> IOTHUBMESSAGE_DISPOSITION_RESULT {
        let client = unsafe { &mut *(ctx as *mut Self) };
        let msg = IotHubMessage::from(message);
        client.callback.on_message(msg).as_raw()
    }

    unsafe extern "C" fn c_input_message_callback(
        message: IOTHUB_MESSAGE_HANDLE,
        ctx: *mut c_void,
    ) -> IOTHUBMESSAGE_DISPOSITION_RESULT {
        let client = unsafe { &mut *(ctx as *mut Self) };
        let msg = IotHubMessage::from(message);
        client.callback.on_input_message(msg).as_raw()
    }

    unsafe extern "C" fn c_connection_status_callback(
        status: IOTHUB_CLIENT_CONNECTION_STATUS,
        reason: IOTHUB_CLIENT_CONNECTION_STATUS_REASON,
        ctx: *mut c_void,
    ) {
        let client = unsafe { &mut *(ctx as *mut Self) };
        client.callback.on_connection_status_changed(status.into(), reason.into());
    }

    unsafe extern "C" fn c_device_twin_callback(
        update_state: DEVICE_TWIN_UPDATE_STATE,
        payload: *const u8,
        payload_length: usize,
        ctx: *mut c_void,
    ) {
        let client = unsafe { &mut *(ctx as *mut Self) };
        let data = unsafe { std::slice::from_raw_parts(payload, payload_length) };
        client
            .callback
            .on_device_twin(update_state.into(), data);
    }

    unsafe extern "C" fn c_device_method_callback(
        method_name: *const c_char,
        payload: *const u8,
        payload_length: usize,
        response_buffer: *mut *mut u8,
        response_buffer_size: *mut usize,
        ctx: *mut c_void,
    ) -> ::std::os::raw::c_int {
        let client = unsafe { &mut *(ctx as *mut Self) };
        let payload = unsafe { std::slice::from_raw_parts(payload, payload_length) };
        let method_name = unsafe { CStr::from_ptr(method_name) }.to_str().unwrap_or_default();
        let result = client
            .callback
            .on_device_method(method_name, payload);
        match result {
            Ok(resp_data) => {
                unsafe {
                    *response_buffer = resp_data.as_ptr() as *mut u8;
                    *response_buffer_size = resp_data.len();
                }
                200
            }
            Err(_) => {
                unsafe {
                    *response_buffer = ptr::null_mut();
                    *response_buffer_size = 0;
                }
                500
            }
        }
    }

    pub fn initialize_callbacks(&mut self) -> Result<(), IotError> {
        let context = self.context_ptr();

        let result = unsafe {
            IoTHubClientCore_SetMessageCallback(
                self.handle,
                Some(Self::c_message_callback),
                context,
            )
        };
        IotError::check_sdk_result(result)?;

        let result = unsafe {
            IoTHubClientCore_SetInputMessageCallback(
                self.handle,
                c"input".as_ptr(),
                Some(Self::c_input_message_callback),
                context,
            )
        };
        IotError::check_sdk_result(result)?;

        let result = unsafe {
            IoTHubClientCore_SetDeviceTwinCallback(
                self.handle,
                Some(Self::c_device_twin_callback),
                context,
            )
        };
        IotError::check_sdk_result(result)?;

        let result = unsafe {
            IoTHubClientCore_SetDeviceMethodCallback(
                self.handle,
                Some(Self::c_device_method_callback),
                context,
            )
        };
        IotError::check_sdk_result(result)?;

        let result = unsafe {
            IoTHubClientCore_SetConnectionStatusCallback(
                self.handle,
                Some(Self::c_connection_status_callback),
                context,
            )
        };
        IotError::check_sdk_result(result)
    }
}

impl<C> IoTHubDeviceClient<C>
where
    C: DeviceClientCallback,
{
    fn ensure_initialized() {
        IOTHUB.call_once(|| unsafe {
            IoTHub_Init();
        });
    }

    unsafe extern "C" fn c_async_confirmation_result_callback(
        result: IOTHUB_CLIENT_CONFIRMATION_RESULT,
        ctx: *mut c_void,
    ) {
        let sender =
            unsafe { Box::from_raw(ctx as *mut oneshot::Sender<IoTHubClientConfirmationResult>) };
        let _ = sender.send(result.into());
    }

    unsafe extern "C" fn c_async_twin_received_callback(
        state: DEVICE_TWIN_UPDATE_STATE,
        payload: *const u8,
        payload_length: usize,
        ctx: *mut c_void,
    ) {
        let sender = unsafe {
            Box::from_raw(ctx as *mut oneshot::Sender<(IoTHubDeviceTwinUpdateState, Vec<u8>)>)
        };
        let data = unsafe { std::slice::from_raw_parts(payload, payload_length) }.to_vec();
        let _ = sender.send((state.into(), data));
    }

    pub fn create_from_connection_string(
        connection_string: &CStr,
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe {
            IoTHubClientCore_CreateFromConnectionString(connection_string.as_ptr(), protocol)
        };
        if handle.is_null() {
            Err(IotError::NullPtr)
        } else {
            Ok(Self { handle, callback })
        }
    }

    pub fn create(config: &IOTHUB_CLIENT_CONFIG, callback: C) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe { IoTHubClientCore_Create(config as *const IOTHUB_CLIENT_CONFIG) };
        if handle.is_null() {
            Err(IotError::NullPtr)
        } else {
            Ok(Self { handle, callback })
        }
    }

    pub fn create_with_transport(
        transport_handle: &Transport,
        config: &IOTHUB_CLIENT_CONFIG,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe {
            IoTHubClientCore_CreateWithTransport(
                transport_handle.as_raw(),
                config as *const IOTHUB_CLIENT_CONFIG,
            )
        };
        if handle.is_null() {
            Err(IotError::NullPtr)
        } else {
            Ok(Self { handle, callback })
        }
    }

    pub fn create_with_transport_config(
        transport_handle: &Transport,
        config: &ConfigBuilder,
        callback: C,
    ) -> Result<Self, IotError> {
        let raw = config.build();
        Self::create_with_transport(transport_handle, &raw, callback)
    }

    pub fn create_from_device_auth(
        iothub_uri: &CStr,
        device_id: &CStr,
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe {
            IoTHubClientCore_CreateFromDeviceAuth(iothub_uri.as_ptr(), device_id.as_ptr(), protocol)
        };
        if handle.is_null() {
            Err(IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
        } else {
            Ok(Self { handle, callback })
        }
    }

    pub fn create_from_environment(
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe { IoTHubClientCore_CreateFromEnvironment(protocol) };
        if handle.is_null() {
            Err(IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
        } else {
            Ok(Self { handle, callback })
        }
    }

    fn destroy(&mut self) {
        unsafe { IoTHubClientCore_Destroy(self.handle) };
    }

    pub async fn send_event_async(
        &self,
        event_message: &IotHubMessage,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        let (sender, receiver) = oneshot::channel::<IoTHubClientConfirmationResult>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubClientCore_SendEventAsync(
                self.handle,
                event_message.handle,
                Some(Self::c_async_confirmation_result_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<IoTHubClientConfirmationResult>,
                ));
            }
            return Err(error);
        }

        receiver
            .await
            .map_err(|_| IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
    }

    pub fn get_send_status(&self) -> Result<IoTHubClientStatus, IotError> {
        let mut status: IOTHUB_CLIENT_STATUS = 0;
        let result = unsafe {
            IoTHubClientCore_GetSendStatus(self.handle, &mut status as *mut IOTHUB_CLIENT_STATUS)
        };
        IotError::check_sdk_result(result)?;
        Ok(status.into())
    }

    pub fn set_retry_policy(
        &self,
        retry_policy: IoTHubClientRetryPolicy,
        retry_timeout_limit_in_seconds: usize,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SetRetryPolicy(
                self.handle,
                retry_policy.as_raw(),
                retry_timeout_limit_in_seconds,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn get_retry_policy(&self) -> Result<(IoTHubClientRetryPolicy, usize), IotError> {
        let mut retry_policy: IOTHUB_CLIENT_RETRY_POLICY = 0;
        let mut retry_timeout_limit_in_seconds: usize = 0;
        let result = unsafe {
            IoTHubClientCore_GetRetryPolicy(
                self.handle,
                &mut retry_policy as *mut IOTHUB_CLIENT_RETRY_POLICY,
                &mut retry_timeout_limit_in_seconds as *mut usize,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok((retry_policy.into(), retry_timeout_limit_in_seconds))
    }

    pub fn get_last_message_receive_time(&self) -> Result<SystemTime, IotError> {
        let mut last_message_receive_time: time_t = 0;
        let result = unsafe {
            IoTHubClientCore_GetLastMessageReceiveTime(
                self.handle,
                &mut last_message_receive_time as *mut time_t,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(UNIX_EPOCH + Duration::from_secs(last_message_receive_time as u64))
    }

    pub fn set_option(&self, option_name: &CStr, value: &[u8]) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SetOption(
                self.handle,
                option_name.as_ptr(),
                value.as_ptr() as *const c_void,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn send_reported_state(
        &mut self,
        reported_state: &[u8],
        reported_state_callback: IOTHUB_CLIENT_REPORTED_STATE_CALLBACK,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SendReportedState(
                self.handle,
                reported_state.as_ptr(),
                reported_state.len(),
                reported_state_callback,
                self.context_ptr(),
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn device_method_response(
        &self,
        method_id: &MethodHandle,
        response: &[u8],
        status_code: ::std::os::raw::c_int,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_DeviceMethodResponse(
                self.handle,
                method_id.as_raw(),
                response.as_ptr(),
                response.len(),
                status_code,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub async fn get_twin_async(&self) -> Result<(IoTHubDeviceTwinUpdateState, Vec<u8>), IotError> {
        let (sender, receiver) = oneshot::channel::<(IoTHubDeviceTwinUpdateState, Vec<u8>)>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubClientCore_GetTwinAsync(
                self.handle,
                Some(Self::c_async_twin_received_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<(IoTHubDeviceTwinUpdateState, Vec<u8>)>,
                ));
            }
            return Err(error);
        }

        receiver
            .await
            .map_err(|_| IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
    }

    pub fn subscribe_to_commands(
        &mut self,
        command_callback: IOTHUB_CLIENT_COMMAND_CALLBACK_ASYNC,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SubscribeToCommands(
                self.handle,
                command_callback,
                self.context_ptr(),
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn initialize_upload(
        &self,
        destination_file_name: &CStr,
    ) -> Result<(NonNull<c_char>, NonNull<c_char>), IotError> {
        let mut upload_correlation_id: *mut c_char = ptr::null_mut();
        let mut azure_blob_sas_uri: *mut c_char = ptr::null_mut();
        let result = unsafe {
            IoTHubClientCore_InitializeUpload(
                self.handle,
                destination_file_name.as_ptr(),
                &mut upload_correlation_id,
                &mut azure_blob_sas_uri,
            )
        };
        IotError::check_sdk_result(result)?;

        match (
            NonNull::new(upload_correlation_id),
            NonNull::new(azure_blob_sas_uri),
        ) {
            (Some(correlation_id), Some(sas_uri)) => Ok((correlation_id, sas_uri)),
            _ => Err(IotError::NullPtr),
        }
    }

    pub fn azure_storage_create_client(
        &self,
        azure_blob_sas_uri: &CStr,
    ) -> Result<UploadContextHandle, IotError> {
        let context = unsafe {
            IoTHubClientCore_AzureStorageCreateClient(self.handle, azure_blob_sas_uri.as_ptr())
        };
        UploadContextHandle::from_raw(context).ok_or(IotError::NullPtr)
    }

    pub fn notify_upload_completion(
        &self,
        upload_correlation_id: &CStr,
        is_success: bool,
        response_code: ::std::os::raw::c_int,
        response_message: Option<&CStr>,
    ) -> Result<(), IotError> {
        let response_message_ptr = response_message.map_or(ptr::null(), |msg| msg.as_ptr());
        let result = unsafe {
            IoTHubClientCore_NotifyUploadCompletion(
                self.handle,
                upload_correlation_id.as_ptr(),
                is_success,
                response_code,
                response_message_ptr,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub async fn send_event_to_output_async(
        &self,
        event_message: &IotHubMessage,
        output_name: &CStr,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        let (sender, receiver) = oneshot::channel::<IoTHubClientConfirmationResult>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubClientCore_SendEventToOutputAsync(
                self.handle,
                event_message.handle,
                output_name.as_ptr(),
                Some(Self::c_async_confirmation_result_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<IoTHubClientConfirmationResult>,
                ));
            }
            return Err(error);
        }

        receiver
            .await
            .map_err(|_| IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
    }

    pub fn set_input_message_callback(
        &mut self,
        input_name: &CStr,
        event_handler_callback: IOTHUB_CLIENT_MESSAGE_CALLBACK_ASYNC,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SetInputMessageCallback(
                self.handle,
                input_name.as_ptr(),
                event_handler_callback,
                self.context_ptr(),
            )
        };
        IotError::check_sdk_result(result)
    }

    pub fn generic_method_invoke(
        &mut self,
        device_id: &CStr,
        module_id: Option<&CStr>,
        method_name: &CStr,
        method_payload: &CStr,
        timeout: ::std::os::raw::c_uint,
        method_invoke_callback: IOTHUB_METHOD_INVOKE_CALLBACK,
    ) -> Result<(), IotError> {
        let module_id_ptr = module_id.map_or(ptr::null(), |m| m.as_ptr());
        let result = unsafe {
            IoTHubClientCore_GenericMethodInvoke(
                self.handle,
                device_id.as_ptr(),
                module_id_ptr,
                method_name.as_ptr(),
                method_payload.as_ptr(),
                timeout,
                method_invoke_callback,
                self.context_ptr(),
            )
        };
        IotError::check_sdk_result(result)
    }

    pub fn send_message_disposition(
        &self,
        message: &IotHubMessage,
        disposition: IoTHubMessageDispositionResult,
    ) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubClientCore_SendMessageDisposition(
                self.handle,
                message.handle,
                disposition.as_raw(),
            )
        };
        IotError::check_sdk_result(result)
    }
}

impl<C> Drop for IoTHubDeviceClient<C> where C : DeviceClientCallback {
    fn drop(&mut self) {
        self.destroy();
    }
}
