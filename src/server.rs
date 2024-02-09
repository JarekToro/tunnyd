use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::errors::Error;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::Docker;
use futures::{Stream, StreamExt};
use russh::server::{Auth, Handle, Msg, Session};
use russh::{server, Channel, ChannelId, CryptoVec};
use russh_keys::key;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::cli::{parse_and_match_args, ContainerArgs};
use crate::docker::find_ssh_enabled_container;
use log::{error, info};

pub struct OutputInputPair {
    output:
        Arc<Mutex<Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
}

pub struct Client {
    session_handle: russh::server::Handle,
    io: Option<OutputInputPair>,
}

#[derive(Clone)]
pub struct Server {
    pub(crate) clients: Arc<Mutex<HashMap<(usize, ChannelId), Client>>>,
    pub(crate) docker: Docker,
    pub(crate) id: usize,
}
fn process_stream(
    channel: ChannelId,
    cloned_handle: Arc<Mutex<Handle>>,
) -> Box<
    dyn Fn(Result<LogOutput, Error>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        + Send
        + 'static,
> {
    Box::new(move |item: Result<LogOutput, Error>| {
        let session_handle_clone = Arc::clone(&cloned_handle);
        Box::pin(async move {
            let handle = session_handle_clone.lock().await;
            match item {
                Ok(data) => {
                    let handle_result = handle
                        .data(channel, CryptoVec::from(data.into_bytes().to_vec()))
                        .await;
                    match handle_result {
                        Ok(_) => println!("Data method success!"),
                        Err(e) => eprintln!("An error occurred: {:?}", e),
                    }
                }
                Err(e) => {
                    handle
                        .data(
                            channel,
                            CryptoVec::from(format!("Error: {}", e).into_bytes().to_vec()),
                        )
                        .await
                        .expect("Failed to send error message");
                }
            }
            drop(handle); // Explicitly drop the lock here
        })
    })
}
impl server::Server for Server {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let cloned_self = self.clone();
        self.id += 1;
        cloned_self
    }
}
impl Server {
    async fn create_and_start_exec(
        &self,
        docker: &Docker,
        args: &ContainerArgs,
        container_id: &str,
    ) -> Result<StartExecResults, anyhow::Error> {
        info!("Creating and starting exec for container {}", container_id);

        let options = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(true),
            cmd: Some(vec!["bash"]),
            tty: Some(true),
            user: args.user.as_ref().map(|s| s.as_str()),
            ..Default::default()
        };

        let exec = match docker.create_exec(container_id, options).await {
            Ok(ex) => {
                info!("Exec created successfully");
                ex
            }
            Err(e) => {
                error!("Failed to create exec: {}", e);
                return Err(e.into());
            }
        };

        let start_options = StartExecOptions {
            detach: false,
            ..Default::default()
        };

        let results = match docker.start_exec(&exec.id, Some(start_options)).await {
            Ok(res) => {
                info!("Exec started successfully");
                res
            }
            Err(e) => {
                error!("Failed to start exec: {}", e);
                return Err(e.into());
            }
        };

        Ok(results)
    }

    async fn handle_output(
        &self,
        process: StartExecResults,
        channel: ChannelId,
        session_handle: Handle,
        client_id: (usize, ChannelId),
    ) {
        match process {
            StartExecResults::Attached { input, output } => {
                self.link_io(channel, session_handle, client_id, input, output)
                    .await;
            }
            _ => {}
        };
    }

    async fn link_io(
        &self,
        channel: ChannelId,
        session_handle: Handle,
        client_id: (usize, ChannelId),
        input: Pin<Box<dyn AsyncWrite + Send>>,
        output: Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>,
    ) {
        let clients = Arc::clone(&self.clients);
        let mut clients_locked = clients.lock().await;
        let client = clients_locked
            .get_mut(&client_id)
            .expect("Client not found");
        let output = Arc::new(Mutex::new(output));
        client.io = Some(OutputInputPair {
            input: input,
            output: Arc::clone(&output),
        });
        let session_handle = Arc::new(Mutex::new(session_handle.clone()));
        let output_clone = Arc::clone(&output);
        let cloned_handle = Arc::clone(&session_handle);
        tokio::spawn(async move {
            let mut locked_output = output_clone.lock().await;
            let stream: &mut Pin<
                Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>,
            > = &mut *locked_output;
            stream
                .for_each(process_stream(channel, cloned_handle))
                .await;
            let cloned_handle_2 = Arc::clone(&session_handle);
            let handle = cloned_handle_2.lock().await;
            handle
                .data(
                    channel,
                    CryptoVec::from("Docker Container exited process \r\n".as_bytes().to_vec()),
                )
                .await
                .expect("TODO: panic message");
            handle.close(channel).await.expect("")
        });
    }
}

#[async_trait]
impl server::Handler for Server {
    type Error = anyhow::Error;

    async fn channel_close(
        self,
        _: ChannelId,
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        Ok((self, session))
    }

    async fn channel_eof(
        self,
        _: ChannelId,
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        Ok((self, session))
    }
    async fn channel_open_session(
        self,
        channel: Channel<Msg>,
        session: Session,
    ) -> Result<(Self, bool, Session), Self::Error> {
        {
            let mut clients = self.clients.lock().await;
            clients.insert(
                (self.id, channel.id()),
                Client {
                    session_handle: session.handle(),
                    io: None,
                },
            );
        }
        Ok((self, true, session))
    }
    async fn channel_open_confirmation(
        self,
        _: ChannelId,
        _: u32,
        _: u32,
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        Ok((self, session))
    }

    async fn exec_request(
        self,
        channel: ChannelId,
        data: &[u8],
        mut session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        let args = parse_and_match_args(data);
        let client_id = (self.id, channel);

        let container_id = match find_ssh_enabled_container(&args).await {
            Ok(t) => t.id.ok_or(anyhow!("Container Id not found")),
            Err(e) => Err(anyhow!(e)),
        };
        match container_id {
            Ok(id) => {
                let process = self
                    .create_and_start_exec(&self.docker, &args, id.as_str())
                    .await?;
                let _ = self
                    .handle_output(process, channel, session.handle(), client_id)
                    .await;
            }
            Err(e) => return Err(e),
        }

        session.request_success();
        session.channel_success(channel);
        Ok((self, session))
    }

    async fn auth_publickey(
        self,
        _: &str,
        _: &key::PublicKey,
    ) -> Result<(Self, server::Auth), Self::Error> {
        // Purposely left this way, don't change or refactor
        Ok((self, server::Auth::Accept))
    }

    async fn auth_none(self, _: &str) -> Result<(Self, Auth), Self::Error> {
        // Purposely left this way, don't change or refactor
        Ok((self, server::Auth::Accept))
    }

    async fn data(
        mut self,
        channel: ChannelId,
        data: &[u8],
        mut session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        {
            // introduced a new scope for the borrow of self
            let client_id = (self.id, channel);
            let clients = Arc::clone(&self.clients);
            let mut locked_clients = clients.lock().await;
            let client = match locked_clients.get_mut(&client_id) {
                Some(c) => c,
                None => return Err(Self::Error::msg("Client Not ready")), // Just an example, replace with the actual error type
            };
            match &mut client.io {
                None => {}
                Some(io) => {
                    // If io.input.write(data) is asynchronous, it should have .await to complete the operation
                    // Also, handle potential errors returned by the write function
                    io.input.write_all(data).await.map_or((), |_| ())
                }
            }
        } // end of self borrow
        session.request_success();
        session.channel_success(channel);
        Ok((self, session))
    }
}
