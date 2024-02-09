use std::collections::HashMap;
use std::sync::Arc;

use russh::*;
use tokio::sync::Mutex;

use crate::docker::connect_to_docker;
use crate::server::Server;

mod cli;
mod docker;
mod server;
#[tokio::main]
async fn main() {
    use tokio::sync::mpsc;
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .init();

    // Assuming the `connect_to_docker` function correctly initializes a `bollard::Docker` instance.
    let docker = connect_to_docker().await.expect("Docker connection failed");

    let config = russh::server::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(3),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(10)),
        keys: vec![russh_keys::key::KeyPair::generate_ed25519().unwrap()],
        methods: MethodSet::NONE,
        ..Default::default()
    };

    let config = Arc::new(config);

    let server = Server {
        clients: Arc::new(Mutex::new(HashMap::new())),
        docker: docker,
        id: 0,
    };

    let (tx, mut rx) = mpsc::channel(1);

    loop {
        let config_clone = config.clone();
        let server_clone = server.clone();
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            match russh::server::run(config_clone, ("0.0.0.0", 2222), server_clone).await {
                Ok(_) => {
                    println!("Server has closed successfully");
                }
                Err(e) => {
                    // Send the error to the receiver
                    tx_clone.send(e).await.unwrap();
                }
            }
        });

        // Only retry if an error occurred, otherwise break the loop
        if rx.recv().await.is_some() {
            println!("Server error occurred. Retrying...");
            continue;
        } else {
            break;
        }
    }
}
