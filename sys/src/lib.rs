#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use super::*;


    #[test]
    fn test_mqtt() {
        unsafe { 
            IoTHub_Init();

            let handle = IoTHubModuleClient_LL_CreateFromEnvironment(Some(MQTT_Protocol));
            IoTHubModuleClient_LL_Destroy(handle);
            
        }
    }

    #[test]
    fn test_amqp() {
        unsafe { 
            IoTHub_Init();

            //let string = CString::new("").unwrap();
            //let transport = IoTHubTransport_Create(Some(), string.as_ptr(), string.as_ptr());
            //let transorthandle = IoTHubTransport_GetLLTransport(transport);
            //let handle = IoTHubModuleClient_CreateFromConnectionString(string.as_ptr(), transporthandle);
            //IoTHubModuleClient_LL_Destroy(handle);
        }
    }

}
