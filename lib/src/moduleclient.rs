use crate::error::IotError;
use crate::message::IotHubMessage;
use crate::transport::TransportProvider;
use crate::{
    ConnectionStatus, ConnectionStatusReason, IoTHubClientConfirmationResult,
    IoTHubClientPropertyPayloadType, IoTHubMessageDispositionResult, IotHubDeviceTwinUpdateState,
};
use azure_iot_rs_sys::*;
use futures::channel::oneshot;
use log::info;
use std::convert::TryInto;
use std::ffi::{CStr, CString, c_void};
use std::future::poll_fn;
use std::os::raw::c_int;
use std::ptr;
use std::result::Result;
use std::str;
use std::sync::Once;
use std::task::Poll;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{thread, time};

static IOTHUB: Once = Once::new();

unsafe extern "C" {
    fn free(ptr: *mut c_void);
}

pub enum ModuleClientOption<'a> {
    LogTrace(bool),
    MessageTimeout(u32),
    ProductInfo(&'a str),
    RetryIntervalSec(u32),
    RetryMaxDelaySecs(u32),
    SasTokenLifetime(u32),
    DoWorkFreqMs(u32),
    AutoUrlEncodeDecode(bool),
    KeepAlive(u32),
    ModelId(&'a str),
}

impl TryInto<&'static CStr> for &ModuleClientOption<'_> {
    type Error = IotError;

    fn try_into(self) -> Result<&'static CStr, Self::Error> {
        let name = match self {
            ModuleClientOption::LogTrace(_) => c"logtrace",
            ModuleClientOption::MessageTimeout(_) => c"messageTimeout",
            ModuleClientOption::ProductInfo(_) => c"product_info",
            ModuleClientOption::RetryIntervalSec(_) => c"retry_interval_sec",
            ModuleClientOption::RetryMaxDelaySecs(_) => c"retry_max_delay_secs",
            ModuleClientOption::SasTokenLifetime(_) => c"sas_token_lifetime",
            ModuleClientOption::DoWorkFreqMs(_) => c"do_work_freq_ms",
            ModuleClientOption::AutoUrlEncodeDecode(_) => c"auto_url_encode_decode",
            ModuleClientOption::KeepAlive(_) => c"keepalive",
            ModuleClientOption::ModelId(_) => c"model_id",
        };
        Ok(name)
    }
}

pub trait ModuleEventCallback {
    fn on_message(&mut self, message: IotHubMessage) -> IoTHubMessageDispositionResult {
        info!("Received message: {:?}", message);
        IoTHubMessageDispositionResult::Accepted
    }

    fn on_module_twin(&mut self, state: IotHubDeviceTwinUpdateState, data: &[u8]) {
        info!(
            "Received module twin update: {state} {}",
            String::from_utf8_lossy(data)
        );
    }

    fn on_module_method(&mut self, method_name: &str, payload: &[u8]) -> Result<Vec<u8>, IotError> {
        info!(
            "Received module method: {} with payload: {}",
            method_name,
            String::from_utf8_lossy(payload)
        );
        Ok(Vec::new())
    }

    fn on_input_message(
        &mut self,
        input_name: &str,
        message: IotHubMessage,
    ) -> IoTHubMessageDispositionResult {
        info!("Received message on input {}: {:?}", input_name, message);
        IoTHubMessageDispositionResult::Accepted
    }

    fn on_connection_status(&mut self, status: ConnectionStatus, reason: ConnectionStatusReason) {
        info!("Connection status changed: {:?} ({:?})", status, reason);
    }

    fn on_confirmation(&mut self, result: Result<(), IotError>) {
        if let Err(error) = result {
            info!("Telemetry confirmation failed: {:?}", error);
        }
    }
}

#[derive(Debug)]
pub struct MethodInvokeResponse {
    pub status: ::std::os::raw::c_int,
    pub payload: Vec<u8>,
}

pub struct IotHubModuleClient<C: ModuleEventCallback> {
    handle: IOTHUB_MODULE_CLIENT_LL_HANDLE,
    callback: C,
}

unsafe impl<C: ModuleEventCallback + Send> Send for IotHubModuleClient<C> {}

// C callbacks.
impl<C: ModuleEventCallback> IotHubModuleClient<C> {
    unsafe extern "C" fn c_message_callback(
        handle: *mut IOTHUB_MESSAGE_HANDLE_DATA_TAG,
        ctx: *mut c_void,
    ) -> IOTHUBMESSAGE_DISPOSITION_RESULT {
        info!("Received message from hub");
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient<C>) };
        let message = IotHubMessage::from_handle(handle);
        client.callback.on_message(message).as_raw()
    }

    unsafe extern "C" fn c_input_message_callback(
        handle: *mut IOTHUB_MESSAGE_HANDLE_DATA_TAG,
        ctx: *mut c_void,
    ) -> IOTHUBMESSAGE_DISPOSITION_RESULT {
        info!("Received message from hub");
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient<C>) };
        let message = IotHubMessage::from_handle(handle);
        client
            .callback
            .on_input_message("default", message)
            .as_raw()
    }

    unsafe extern "C" fn c_connection_status_callback(
        status: IOTHUB_CLIENT_CONNECTION_STATUS,
        reason: IOTHUB_CLIENT_CONNECTION_STATUS_REASON,
        ctx: *mut c_void,
    ) {
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient<C>) };
        client
            .callback
            .on_connection_status(status.into(), reason.into());
    }

    unsafe extern "C" fn c_module_twin_callback(
        state: DEVICE_TWIN_UPDATE_STATE,
        payload: *const u8,
        size: usize,
        ctx: *mut c_void,
    ) {
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient<C>) };
        let data = unsafe { std::slice::from_raw_parts(payload, size) };
        client.callback.on_module_twin(state.into(), data);
    }

    unsafe extern "C" fn c_module_method_callback(
        method_name: *const i8,
        payload: *const u8,
        size: usize,
        response_payload: *mut *mut u8,
        response_payload_size: *mut usize,
        ctx: *mut c_void,
    ) -> ::std::os::raw::c_int {
        let client = unsafe { &mut *(ctx as *mut IotHubModuleClient<C>) };
        let command_name = unsafe { CStr::from_ptr(method_name) }
            .to_str()
            .unwrap_or_default();
        let data = unsafe { std::slice::from_raw_parts(payload, size) };
        let response = client.callback.on_module_method(command_name, data);
        match response {
            Ok(resp_data) => {
                unsafe {
                    *response_payload = resp_data.as_ptr() as *mut u8;
                    *response_payload_size = resp_data.len();
                }
                200
            }
            Err(_) => {
                unsafe {
                    *response_payload = ptr::null_mut();
                    *response_payload_size = 0;
                }
                500
            }
        }
    }

    fn initialize_callbacks(&mut self) -> Result<(), IotError> {
        let handle = self.handle;
        let context = self.context_ptr();
        unsafe {
            let input = c"input";
            let result = IoTHubModuleClient_LL_SetInputMessageCallback(
                handle,
                input.as_ptr(),
                Some(Self::c_input_message_callback),
                context,
            );
            IotError::check_sdk_result(result)?;

            let result = IoTHubModuleClient_LL_SetMessageCallback(
                handle,
                Some(Self::c_message_callback),
                context,
            );
            IotError::check_sdk_result(result)?;

            let result = IoTHubModuleClient_LL_SetModuleMethodCallback(
                handle,
                Some(Self::c_module_method_callback),
                context,
            );
            IotError::check_sdk_result(result)?;

            let result = IoTHubModuleClient_LL_SetModuleTwinCallback(
                handle,
                Some(Self::c_module_twin_callback),
                context,
            );
            IotError::check_sdk_result(result)?;

            let result = IoTHubModuleClient_LL_SetConnectionStatusCallback(
                handle,
                Some(Self::c_connection_status_callback),
                context,
            );
            IotError::check_sdk_result(result)
        }
    }
}

// Async callbacks.
impl<C: ModuleEventCallback> IotHubModuleClient<C> {
    unsafe extern "C" fn c_async_confirmation_callback(
        status: IOTHUB_CLIENT_RESULT,
        ctx: *mut c_void,
    ) {
        let sender = unsafe { Box::from_raw(ctx as *mut oneshot::Sender<Result<(), IotError>>) };
        let _ = sender.send(IotError::check_sdk_result(status));
    }
    unsafe extern "C" fn c_async_confirmation_result_callback(
        result: IOTHUB_CLIENT_CONFIRMATION_RESULT,
        ctx: *mut c_void,
    ) {
        let sender =
            unsafe { Box::from_raw(ctx as *mut oneshot::Sender<IoTHubClientConfirmationResult>) };
        let _ = sender.send(result.into());
    }

    unsafe extern "C" fn c_async_property_ack_callback(status_code: c_int, ctx: *mut c_void) {
        let sender = unsafe { Box::from_raw(ctx as *mut oneshot::Sender<c_int>) };
        let _ = sender.send(status_code);
    }

    unsafe extern "C" fn c_async_properties_received_callback(
        payload_type: IOTHUB_CLIENT_PROPERTY_PAYLOAD_TYPE,
        payload: *const u8,
        payload_length: usize,
        ctx: *mut c_void,
    ) {
        let sender = unsafe {
            Box::from_raw(ctx as *mut oneshot::Sender<(IoTHubClientPropertyPayloadType, Vec<u8>)>)
        };
        let data = unsafe { std::slice::from_raw_parts(payload, payload_length) }.to_vec();
        let _ = sender.send((payload_type.into(), data));
    }

    unsafe extern "C" fn c_get_twin_async_callback(
        state: DEVICE_TWIN_UPDATE_STATE,
        payload: *const u8,
        size: usize,
        ctx: *mut c_void,
    ) {
        let sender = unsafe {
            Box::from_raw(ctx as *mut oneshot::Sender<(IotHubDeviceTwinUpdateState, Vec<u8>)>)
        };
        let data = unsafe { std::slice::from_raw_parts(payload, size) }.to_vec();
        let _ = sender.send((state.into(), data));
    }
}

impl<C: ModuleEventCallback> IotHubModuleClient<C> {
    fn ensure_initialized() {
        IOTHUB.call_once(|| unsafe {
            IoTHub_Init();
        });
    }

    fn context_ptr(&mut self) -> *mut c_void {
        self as *mut IotHubModuleClient<C> as *mut c_void
    }

    pub fn create_from_environment(
        protocol: TransportProvider,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let handle = unsafe { IoTHubModuleClient_LL_CreateFromEnvironment(protocol.to_sdk()) };
        if handle.is_null() {
            return Err(IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR));
        }

        let mut client = Self { handle, callback };

        client.initialize_callbacks()?;
        Ok(client)
    }

    pub fn create_from_connection_string(
        connection_string: &str,
        transport: TransportProvider,
        callback: C,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let connection = CString::new(connection_string)?;
        let handle = unsafe {
            IoTHubModuleClient_LL_CreateFromConnectionString(
                connection.as_ptr(),
                transport.to_sdk(),
            )
        };
        if handle.is_null() {
            return Err(IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR));
        }
        let mut client = Self { handle, callback };
        client.initialize_callbacks()?;
        Ok(client)
    }

    pub fn try_new(callback: C) -> Result<Self, IotError> {
        Self::create_from_environment(TransportProvider::Mqtt, callback)
    }

    pub async fn send_event_async(&mut self, message: &IotHubMessage) -> Result<(), IotError> {
        let (sender, mut receiver) = oneshot::channel::<Result<(), IotError>>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_SendEventAsync(
                self.handle,
                message.handle,
                Some(Self::c_async_confirmation_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<Result<(), IotError>>,
                ));
            }
            return Err(error);
        }

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(send_result)) => Poll::Ready(send_result),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub async fn send_message_result(
        &mut self,
        message: &IotHubMessage,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        let output = c"output";
        self.send_event_to_output_async(&message, output).await
    }

    pub async fn send_message(
        &mut self,
        message: &IotHubMessage,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        self.send_message_result(message).await
    }

    pub fn get_send_status(&self) -> Result<(), IotError> {
        let handle = self.handle;
        let mut status: IOTHUB_CLIENT_STATUS = 0;
        let result = unsafe { IoTHubModuleClient_LL_GetSendStatus(handle, &mut status) };
        IotError::check_sdk_result(result)?;
        IotError::check_sdk_result(status)
    }

    pub fn set_retry_policy(
        &self,
        retry_policy: IOTHUB_CLIENT_RETRY_POLICY,
        retry_timeout_limit_in_seconds: usize,
    ) -> Result<(), IotError> {
        let handle = self.handle;
        let result = unsafe {
            IoTHubModuleClient_LL_SetRetryPolicy(
                handle,
                retry_policy,
                retry_timeout_limit_in_seconds,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub fn get_retry_policy(&self) -> Result<(IOTHUB_CLIENT_RETRY_POLICY, usize), IotError> {
        let handle = self.handle;
        let mut retry_policy: IOTHUB_CLIENT_RETRY_POLICY = 0;
        let mut timeout_limit: usize = 0;
        let result = unsafe {
            IoTHubModuleClient_LL_GetRetryPolicy(handle, &mut retry_policy, &mut timeout_limit)
        };
        IotError::check_sdk_result(result)?;
        Ok((retry_policy, timeout_limit))
    }

    pub fn get_last_message_receive_time(&self) -> Result<SystemTime, IotError> {
        let handle = self.handle;
        let mut receive_time: time_t = 0;
        let result =
            unsafe { IoTHubModuleClient_LL_GetLastMessageReceiveTime(handle, &mut receive_time) };
        IotError::check_sdk_result(result)?;
        let time = UNIX_EPOCH + Duration::from_secs(receive_time as u64);
        Ok(time)
    }

    pub fn do_work_once(&self) -> Result<(), IotError> {
        let handle = self.handle;
        unsafe {
            IoTHubModuleClient_LL_DoWork(handle);
        }
        Ok(())
    }

    pub fn do_work(&mut self) {
        loop {
            let _ = self.do_work_once();
            thread::sleep(time::Duration::from_millis(100));
        }
    }

    pub fn set_option_value<T>(&self, option_name: &str, value: &T) -> Result<(), IotError> {
        let handle = self.handle;
        let name = CString::new(option_name)?;
        let result = unsafe {
            IoTHubModuleClient_LL_SetOption(
                handle,
                name.as_ptr(),
                value as *const T as *const c_void,
            )
        };
        IotError::check_sdk_result(result)
    }

    fn set_bool_option(&self, name: &CStr, value: bool) -> Result<(), IotError> {
        let value = if value { 1 } else { 0 };
        let result = unsafe {
            IoTHubModuleClient_LL_SetOption(
                self.handle,
                name.as_ptr(),
                &value as *const i32 as *const c_void,
            )
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    fn set_int_option(&self, name: &CStr, value: u32) -> Result<(), IotError> {
        let result = unsafe {
            IoTHubModuleClient_LL_SetOption(
                self.handle,
                name.as_ptr(),
                &value as *const u32 as *const c_void,
            )
        };
        IotError::check_sdk_result(result)
    }

    fn set_str_option(&self, name: &CStr, value: &str) -> Result<(), IotError> {
        let cvalue = CString::new(value)?;
        let result = unsafe {
            IoTHubModuleClient_LL_SetOption(
                self.handle,
                name.as_ptr(),
                cvalue.as_ptr() as *const c_void,
            )
        };
        IotError::check_sdk_result(result)
    }

    pub fn send_reported_state(
        &mut self,
        reported_state: &[u8],
        callback: IOTHUB_CLIENT_REPORTED_STATE_CALLBACK,
    ) -> Result<(), IotError> {
        let handle = self.handle;
        let result = unsafe {
            IoTHubModuleClient_LL_SendReportedState(
                handle,
                reported_state.as_ptr(),
                reported_state.len(),
                callback,
                self.context_ptr(),
            )
        };
        IotError::check_sdk_result(result)
    }

    pub async fn get_twin_async(
        &mut self,
    ) -> Result<(IotHubDeviceTwinUpdateState, Vec<u8>), IotError> {
        let (sender, mut receiver) = oneshot::channel::<(IotHubDeviceTwinUpdateState, Vec<u8>)>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_GetTwinAsync(
                self.handle,
                Some(Self::c_get_twin_async_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<(IotHubDeviceTwinUpdateState, Vec<u8>)>,
                ));
            }
            return Err(error);
        }

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(twin_update)) => Poll::Ready(Ok(twin_update)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub async fn send_event_to_output_async(
        &mut self,
        message: &IotHubMessage,
        output_name: &CStr,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        let (sender, mut receiver) = oneshot::channel::<IoTHubClientConfirmationResult>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_SendEventToOutputAsync(
                self.handle,
                message.handle,
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

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(confirmation)) => Poll::Ready(Ok(confirmation)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub fn device_method_invoke(
        &self,
        device_id: &str,
        method_name: &str,
        method_payload: &str,
        timeout: Duration,
    ) -> Result<MethodInvokeResponse, IotError> {
        let handle = self.handle;
        let device_id = CString::new(device_id)?;
        let method_name = CString::new(method_name)?;
        let method_payload = CString::new(method_payload)?;

        let mut response_status: ::std::os::raw::c_int = 0;
        let mut response_payload: *mut ::std::os::raw::c_uchar = ptr::null_mut();
        let mut response_payload_size: usize = 0;

        let result = unsafe {
            IoTHubModuleClient_LL_DeviceMethodInvoke(
                handle,
                device_id.as_ptr(),
                method_name.as_ptr(),
                method_payload.as_ptr(),
                timeout.as_secs() as ::std::os::raw::c_uint,
                &mut response_status,
                &mut response_payload,
                &mut response_payload_size,
            )
        };

        if result != IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK && !response_payload.is_null() {
            unsafe { free(response_payload as *mut c_void) };
        }
        IotError::check_sdk_result(result)?;

        let payload = if response_payload.is_null() || response_payload_size == 0 {
            Vec::new()
        } else {
            let bytes =
                unsafe { std::slice::from_raw_parts(response_payload, response_payload_size) }
                    .to_vec();
            unsafe { free(response_payload as *mut c_void) };
            bytes
        };

        Ok(MethodInvokeResponse {
            status: response_status,
            payload,
        })
    }

    pub fn module_method_invoke(
        &self,
        device_id: &str,
        module_id: &str,
        method_name: &str,
        method_payload: &str,
        timeout: ::std::os::raw::c_uint,
    ) -> Result<MethodInvokeResponse, IotError> {
        let handle = self.handle;
        let device_id = CString::new(device_id)?;
        let module_id = CString::new(module_id)?;
        let method_name = CString::new(method_name)?;
        let method_payload = CString::new(method_payload)?;

        let mut response_status: ::std::os::raw::c_int = 0;
        let mut response_payload: *mut ::std::os::raw::c_uchar = ptr::null_mut();
        let mut response_payload_size: usize = 0;

        let result = unsafe {
            IoTHubModuleClient_LL_ModuleMethodInvoke(
                handle,
                device_id.as_ptr(),
                module_id.as_ptr(),
                method_name.as_ptr(),
                method_payload.as_ptr(),
                timeout,
                &mut response_status,
                &mut response_payload,
                &mut response_payload_size,
            )
        };

        if result != IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK && !response_payload.is_null() {
            unsafe { free(response_payload as *mut c_void) };
        }
        IotError::check_sdk_result(result)?;

        let payload = if response_payload.is_null() || response_payload_size == 0 {
            Vec::new()
        } else {
            let bytes =
                unsafe { std::slice::from_raw_parts(response_payload, response_payload_size) }
                    .to_vec();
            unsafe { free(response_payload as *mut c_void) };
            bytes
        };

        Ok(MethodInvokeResponse {
            status: response_status,
            payload,
        })
    }

    pub fn send_message_disposition(
        &self,
        message: &IotHubMessage,
        disposition: IOTHUBMESSAGE_DISPOSITION_RESULT,
    ) -> Result<(), IotError> {
        let handle = self.handle;
        let result = unsafe {
            IoTHubModuleClient_LL_SendMessageDisposition(handle, message.handle, disposition)
        };
        IotError::check_sdk_result(result)?;
        Ok(())
    }

    pub async fn send_telemetry_async(
        &mut self,
        message: &IotHubMessage,
    ) -> Result<IoTHubClientConfirmationResult, IotError> {
        let (sender, mut receiver) = oneshot::channel::<IoTHubClientConfirmationResult>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_SendTelemetryAsync(
                self.handle,
                message.handle,
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

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(confirmation)) => Poll::Ready(Ok(confirmation)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub fn subscribe_to_commands(
        &mut self,
        callback: IOTHUB_CLIENT_COMMAND_CALLBACK_ASYNC,
    ) -> Result<(), IotError> {
        let handle = self.handle;
        let result = unsafe {
            IoTHubModuleClient_LL_SubscribeToCommands(handle, callback, self.context_ptr())
        };
        IotError::check_sdk_result(result)
    }

    pub async fn send_properties_async(&mut self, properties: &[u8]) -> Result<c_int, IotError> {
        let (sender, mut receiver) = oneshot::channel::<c_int>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_SendPropertiesAsync(
                self.handle,
                properties.as_ptr(),
                properties.len(),
                Some(Self::c_async_property_ack_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(context as *mut oneshot::Sender<c_int>));
            }
            return Err(error);
        }

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(status_code)) => Poll::Ready(Ok(status_code)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub async fn get_properties_async(
        &mut self,
    ) -> Result<(IoTHubClientPropertyPayloadType, Vec<u8>), IotError> {
        let (sender, mut receiver) =
            oneshot::channel::<(IoTHubClientPropertyPayloadType, Vec<u8>)>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_GetPropertiesAsync(
                self.handle,
                Some(Self::c_async_properties_received_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<(IoTHubClientPropertyPayloadType, Vec<u8>)>,
                ));
            }
            return Err(error);
        }

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(properties)) => Poll::Ready(Ok(properties)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub async fn get_properties_and_subscribe_to_updates_async(
        &mut self,
    ) -> Result<(IoTHubClientPropertyPayloadType, Vec<u8>), IotError> {
        let (sender, mut receiver) =
            oneshot::channel::<(IoTHubClientPropertyPayloadType, Vec<u8>)>();
        let context = Box::into_raw(Box::new(sender)) as *mut c_void;

        let result = unsafe {
            IoTHubModuleClient_LL_GetPropertiesAndSubscribeToUpdatesAsync(
                self.handle,
                Some(Self::c_async_properties_received_callback),
                context,
            )
        };

        if let Err(error) = IotError::check_sdk_result(result) {
            unsafe {
                drop(Box::from_raw(
                    context as *mut oneshot::Sender<(IoTHubClientPropertyPayloadType, Vec<u8>)>,
                ));
            }
            return Err(error);
        }

        poll_fn(|cx| {
            self.do_work_once()?;

            match receiver.try_recv() {
                Ok(Some(properties)) => Poll::Ready(Ok(properties)),
                Ok(None) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(Err(IotError::Sdk(
                    IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR,
                ))),
            }
        })
        .await
    }

    pub fn set_option(&self, option: ModuleClientOption<'_>) -> Result<(), IotError> {
        let name: &CStr = (&option).try_into()?;
        match option {
            ModuleClientOption::LogTrace(value)
            | ModuleClientOption::AutoUrlEncodeDecode(value) => {
                self.set_bool_option(name, value)?
            }
            ModuleClientOption::MessageTimeout(value)
            | ModuleClientOption::RetryIntervalSec(value)
            | ModuleClientOption::RetryMaxDelaySecs(value)
            | ModuleClientOption::SasTokenLifetime(value)
            | ModuleClientOption::DoWorkFreqMs(value)
            | ModuleClientOption::KeepAlive(value) => self.set_int_option(name, value)?,
            ModuleClientOption::ProductInfo(value) | ModuleClientOption::ModelId(value) => {
                self.set_str_option(name, value)?
            }
        }

        Ok(())
    }
}

impl<C: ModuleEventCallback> Drop for IotHubModuleClient<C> {
    fn drop(&mut self) {
        unsafe {
            IoTHubModuleClient_LL_Destroy(self.handle);
        }
    }
}
