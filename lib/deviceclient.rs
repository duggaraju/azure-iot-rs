use crate::error::IotError;
use crate::message::IotHubMessage;
use crate::transport::Transport;
use azure_iot_rs_sys::*;
use std::ffi::{CStr, c_char, c_void};
use std::ptr::{self, NonNull};
use std::sync::Once;

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

pub struct IoTHubDeviceClient {
    handle: IOTHUB_CLIENT_CORE_HANDLE,
}

impl IoTHubDeviceClient {
    fn ensure_initialized() {
        IOTHUB.call_once(|| unsafe {
            IoTHub_Init();
        });
    }

    fn context_ptr(context: Option<NonNull<c_void>>) -> *mut c_void {
        context.map_or(ptr::null_mut(), |ptr| ptr.as_ptr())
    }

    pub fn create_from_connection_string(
        connection_string: &str,
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
    ) -> Result<Self, IotError> {
        Self::ensure_initialized();
        let connection = CString::new(connection_string)?;
        let handle = unsafe {
            IoTHubClientCore_CreateFromConnectionString(connection.as_ptr(), protocol)
        };
        if handle.is_null() {
            Err(IotError::Sdk(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR))
        } else {
            Ok(Self { handle })
        }
    }

    pub fn create(config: &IOTHUB_CLIENT_CONFIG) -> Option<Self> {
        Self::ensure_initialized();
        let handle = unsafe { IoTHubClientCore_Create(config as *const IOTHUB_CLIENT_CONFIG) };
        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    pub fn create_with_transport(
        transport_handle: &Transport,
        config: &IOTHUB_CLIENT_CONFIG,
    ) -> Option<Self> {
        Self::ensure_initialized();
        let handle = unsafe {
            IoTHubClientCore_CreateWithTransport(
                transport_handle.as_raw(),
                config as *const IOTHUB_CLIENT_CONFIG,
            )
        };
        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    pub fn create_from_device_auth(
        iothub_uri: &CStr,
        device_id: &CStr,
        protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER,
    ) -> Option<Self> {
        Self::ensure_initialized();
        let handle = unsafe {
            IoTHubClientCore_CreateFromDeviceAuth(iothub_uri.as_ptr(), device_id.as_ptr(), protocol)
        };
        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    pub fn create_from_environment(protocol: IOTHUB_CLIENT_TRANSPORT_PROVIDER) -> Option<Self> {
        Self::ensure_initialized();
        let handle = unsafe { IoTHubClientCore_CreateFromEnvironment(protocol) };
        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    fn destroy(&mut self) {
        unsafe { IoTHubClientCore_Destroy(self.handle) };
    }

    pub fn send_event_async(
        &self,
        event_message: &IotHubMessage,
        event_confirmation_callback: IOTHUB_CLIENT_EVENT_CONFIRMATION_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SendEventAsync(
                handle,
                event_message.handle,
                event_confirmation_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn get_send_status(&self) -> Result<IOTHUB_CLIENT_STATUS, IOTHUB_CLIENT_RESULT> {
        let handle = self.handle;
        let mut status: IOTHUB_CLIENT_STATUS = 0;
        let result = unsafe {
            IoTHubClientCore_GetSendStatus(handle, &mut status as *mut IOTHUB_CLIENT_STATUS)
        };
        if result == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK {
            Ok(status)
        } else {
            Err(result)
        }
    }

    pub fn set_message_callback(
        &self,
        message_callback: IOTHUB_CLIENT_MESSAGE_CALLBACK_ASYNC,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetMessageCallback(
                handle,
                message_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn set_connection_status_callback(
        &self,
        connection_status_callback: IOTHUB_CLIENT_CONNECTION_STATUS_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetConnectionStatusCallback(
                handle,
                connection_status_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn set_retry_policy(
        &self,
        retry_policy: IOTHUB_CLIENT_RETRY_POLICY,
        retry_timeout_limit_in_seconds: usize,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetRetryPolicy(handle, retry_policy, retry_timeout_limit_in_seconds)
        }
    }

    pub fn get_retry_policy(
        &self,
    ) -> Result<(IOTHUB_CLIENT_RETRY_POLICY, usize), IOTHUB_CLIENT_RESULT> {
        let handle = self.handle;
        let mut retry_policy: IOTHUB_CLIENT_RETRY_POLICY = 0;
        let mut retry_timeout_limit_in_seconds: usize = 0;
        let result = unsafe {
            IoTHubClientCore_GetRetryPolicy(
                handle,
                &mut retry_policy as *mut IOTHUB_CLIENT_RETRY_POLICY,
                &mut retry_timeout_limit_in_seconds as *mut usize,
            )
        };
        if result == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK {
            Ok((retry_policy, retry_timeout_limit_in_seconds))
        } else {
            Err(result)
        }
    }

    pub fn get_last_message_receive_time(&self) -> Result<time_t, IOTHUB_CLIENT_RESULT> {
        let handle = self.handle;
        let mut last_message_receive_time: time_t = 0;
        let result = unsafe {
            IoTHubClientCore_GetLastMessageReceiveTime(
                handle,
                &mut last_message_receive_time as *mut time_t,
            )
        };
        if result == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK {
            Ok(last_message_receive_time)
        } else {
            Err(result)
        }
    }

    pub fn set_option(&self, option_name: &CStr, value: &[u8]) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetOption(
                handle,
                option_name.as_ptr(),
                value.as_ptr() as *const c_void,
            )
        }
    }

    pub fn set_device_twin_callback(
        &self,
        device_twin_callback: IOTHUB_CLIENT_DEVICE_TWIN_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetDeviceTwinCallback(
                handle,
                device_twin_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn send_reported_state(
        &self,
        reported_state: &[u8],
        reported_state_callback: IOTHUB_CLIENT_REPORTED_STATE_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SendReportedState(
                handle,
                reported_state.as_ptr(),
                reported_state.len(),
                reported_state_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn set_device_method_callback(
        &self,
        device_method_callback: IOTHUB_CLIENT_DEVICE_METHOD_CALLBACK_ASYNC,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetDeviceMethodCallback(
                handle,
                device_method_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn set_device_method_callback_ex(
        &self,
        inbound_device_method_callback: IOTHUB_CLIENT_INBOUND_DEVICE_METHOD_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetDeviceMethodCallback_Ex(
                handle,
                inbound_device_method_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn device_method_response(
        &self,
        method_id: &MethodHandle,
        response: &[u8],
        status_code: ::std::os::raw::c_int,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_DeviceMethodResponse(
                handle,
                method_id.as_raw(),
                response.as_ptr(),
                response.len(),
                status_code,
            )
        }
    }

    pub fn get_twin_async(
        &self,
        device_twin_callback: IOTHUB_CLIENT_DEVICE_TWIN_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_GetTwinAsync(
                handle,
                device_twin_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn subscribe_to_commands(
        &self,
        command_callback: IOTHUB_CLIENT_COMMAND_CALLBACK_ASYNC,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SubscribeToCommands(
                handle,
                command_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn initialize_upload(
        &self,
        destination_file_name: &CStr,
    ) -> Result<(NonNull<c_char>, NonNull<c_char>), IOTHUB_CLIENT_RESULT> {
        let handle = self.handle;
        let mut upload_correlation_id: *mut c_char = ptr::null_mut();
        let mut azure_blob_sas_uri: *mut c_char = ptr::null_mut();
        let result = unsafe {
            IoTHubClientCore_InitializeUpload(
                handle,
                destination_file_name.as_ptr(),
                &mut upload_correlation_id,
                &mut azure_blob_sas_uri,
            )
        };
        if result == IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_OK {
            match (
                NonNull::new(upload_correlation_id),
                NonNull::new(azure_blob_sas_uri),
            ) {
                (Some(correlation_id), Some(sas_uri)) => Ok((correlation_id, sas_uri)),
                _ => Err(IOTHUB_CLIENT_RESULT_TAG_IOTHUB_CLIENT_ERROR),
            }
        } else {
            Err(result)
        }
    }

    pub fn azure_storage_create_client(
        &self,
        azure_blob_sas_uri: &CStr,
    ) -> Result<UploadContextHandle, IotError> {
        let handle = self.handle;
        let context = unsafe {
            IoTHubClientCore_AzureStorageCreateClient(handle, azure_blob_sas_uri.as_ptr())
        };
        UploadContextHandle::from_raw(context).ok_or(IotError::Null())
    }

    pub fn upload_to_blob_async(
        &self,
        destination_file_name: &CStr,
        source: &[u8],
        iothub_client_file_upload_callback: IOTHUB_CLIENT_FILE_UPLOAD_CALLBACK,
        context: Option<NonNull<c_void>>,
    ) -> Result<(), IotError> {
        let restult = unsafe {
            IoTHubClientCore_UploadToBlobAsync(
                self.0,
                destination_file_name.as_ptr(),
                source.as_ptr(),
                source.len(),
                iothub_client_file_upload_callback,
                Self::context_ptr(context),
            )
        };

    }

    pub fn upload_multiple_blocks_to_blob_async(
        &self,
        destination_file_name: &CStr,
        get_data_callback: IOTHUB_CLIENT_FILE_UPLOAD_GET_DATA_CALLBACK,
        get_data_callback_ex: IOTHUB_CLIENT_FILE_UPLOAD_GET_DATA_CALLBACK_EX,
        context: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        unsafe {
            IoTHubClientCore_UploadMultipleBlocksToBlobAsync(
                self.0,
                destination_file_name.as_ptr(),
                get_data_callback,
                get_data_callback_ex,
                Self::context_ptr(context),
            )
        }
    }

    pub fn notify_upload_completion(
        &self,
        upload_correlation_id: &CStr,
        is_success: bool,
        response_code: ::std::os::raw::c_int,
        response_message: Option<&CStr>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        let response_message_ptr = response_message.map_or(ptr::null(), |msg| msg.as_ptr());
        unsafe {
            IoTHubClientCore_NotifyUploadCompletion(
                handle,
                upload_correlation_id.as_ptr(),
                is_success,
                response_code,
                response_message_ptr,
            )
        }
    }

    pub fn send_event_to_output_async(
        &self,
        event_message: &IotHubMessage,
        output_name: &CStr,
        event_confirmation_callback: IOTHUB_CLIENT_EVENT_CONFIRMATION_CALLBACK,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SendEventToOutputAsync(
                handle,
                event_message.handle,
                output_name.as_ptr(),
                event_confirmation_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn set_input_message_callback(
        &self,
        input_name: &CStr,
        event_handler_callback: IOTHUB_CLIENT_MESSAGE_CALLBACK_ASYNC,
        user_context_callback: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe {
            IoTHubClientCore_SetInputMessageCallback(
                handle,
                input_name.as_ptr(),
                event_handler_callback,
                Self::context_ptr(user_context_callback),
            )
        }
    }

    pub fn generic_method_invoke(
        &self,
        device_id: &CStr,
        module_id: Option<&CStr>,
        method_name: &CStr,
        method_payload: &CStr,
        timeout: ::std::os::raw::c_uint,
        method_invoke_callback: IOTHUB_METHOD_INVOKE_CALLBACK,
        context: Option<NonNull<c_void>>,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        let module_id_ptr = module_id.map_or(ptr::null(), |m| m.as_ptr());
        unsafe {
            IoTHubClientCore_GenericMethodInvoke(
                handle,
                device_id.as_ptr(),
                module_id_ptr,
                method_name.as_ptr(),
                method_payload.as_ptr(),
                timeout,
                method_invoke_callback,
                Self::context_ptr(context),
            )
        }
    }

    pub fn send_message_disposition(
        &self,
        message: &IotHubMessage,
        disposition: IOTHUBMESSAGE_DISPOSITION_RESULT,
    ) -> IOTHUB_CLIENT_RESULT {
        let handle = self.handle;
        unsafe { IoTHubClientCore_SendMessageDisposition(handle, message.handle, disposition) }
    }
}

impl Drop for IoTHubDeviceClient {
    fn drop(&mut self) {
        self.destroy();
    }
}
