// Simple example showing how to call into the generated bindings.
// Build and run from repository root:
//   cargo run --example basic
//
// Run it from the workspace root with:
//   cargo run -p azure-iot-rs --example basic

use azure_iot_rs::MessageBody;
use azure_iot_rs::{IotHubModuleClient, IotHubModuleEvent};

fn main() {
    println!("azure-iot-rs example CLI — starting client...");

    let mut client = IotHubModuleClient::new(move |event| match event {
        IotHubModuleEvent::Message(msg) => match msg.body() {
            MessageBody::Text(s) => println!("Received message text: {}", s),
            MessageBody::Binary(b) => println!("Received message binary ({:?} {} bytes)",b, b.len()),
        },
        IotHubModuleEvent::Twin(twin) => println!("Received twin update: {}", twin),
    });

    println!("Client initialized. Entering work loop (Ctrl+C to exit)...");
    client.do_work();
}
