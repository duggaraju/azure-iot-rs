#[macro_use]
extern crate log;

pub mod message;
pub mod module;


#[cfg(test)]
mod tests {

    use super::module::*;
    #[test]
    fn initialize_client() {
        let _client = IotHubModuleClient::new(move |_event| { info!("Received event!"); });
    }
}
