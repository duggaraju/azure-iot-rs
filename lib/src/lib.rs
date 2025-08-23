mod message;
mod module;

pub use message::{IotHubMessage, MessageBody};
pub use module::{IotHubModuleClient, IotHubModuleEvent};

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn initialize_client() {
        let _client = IotHubModuleClient::new(move |_event| {
            info!("Received event!");
        });
    }
}
