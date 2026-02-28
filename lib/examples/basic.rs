// Simple example showing how to call into the generated bindings.
// Build and run from repository root:
//   cargo run --example basic
//
// Run it from the workspace root with:
//   cargo run -p azure-iot-rs --example basic

use azure_iot_rs::MessageBody;
use azure_iot_rs::{IoTHubMessageDispositionResult, IotHubModuleClient, ModuleEventCallback};

struct PrintModuleEvents;

impl ModuleEventCallback for PrintModuleEvents {
    fn on_message(&mut self, msg: azure_iot_rs::IotHubMessage) -> IoTHubMessageDispositionResult {
        match msg.body() {
            MessageBody::Text(s) => println!("Received message text: {s}"),
            MessageBody::Binary(b) => {
                println!("Received message binary ({:?} {} bytes)", b, b.len())
            }
        }
        IoTHubMessageDispositionResult::Accepted
    }

    fn on_module_twin(&mut self, state: azure_iot_rs::IotHubDeviceTwinUpdateState, data: &[u8]) {
        println!(
            "Received twin update ({state}): {}",
            String::from_utf8_lossy(data)
        );
    }

    fn on_confirmation(&mut self, _status: Result<(), azure_iot_rs::IotError>) {}
}

fn main() {
    println!("azure-iot-rs example CLI — starting client...");

    let mut client = IotHubModuleClient::try_new(PrintModuleEvents).unwrap();

    println!("Client initialized. Entering work loop (Ctrl+C to exit)...");
    client.do_work();
}
