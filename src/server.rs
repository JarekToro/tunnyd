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

/// Represents a pair of output and input streams.
///
/// # Remarks
///
/// - The `output` field is a shared, thread-safe, mutable reference to a stream of log outputs.
/// - The `input` field is a pinned, boxed, asynchronous write trait object which can be safely
///   sent across threads.
pub struct OutputInputPair {
    output:
        Arc<Mutex<Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
}

/// Represents a SSH client.
///
/// # Fields
///
/// - `session_handle`: A handle to the SSH session.
/// - `io`: Optional pair of output and input streams.
///
/// # Remarks
///
/// - The `session_handle` field provides access to the session functionality of the SSH client,
///   allowing the execution of commands, shell access, and file transfer.
/// - The `io` field is an optional pair of output and input streams used for interacting with the SSH
///   client. If `None`, the client does not have any associated streams.
pub struct Client {
    session_handle: russh::server::Handle,
    io: Option<OutputInputPair>,
}

/// Represents an ssh server.
///
/// # Remarks
///
/// - The `clients` field is a shared, thread-safe, mutable reference to a hash map storing the
///   clients connected to the server.
/// - The `docker` field is an instance of the `bollard::docker` struct, representing the Docker api
///   associated with the server.
/// - The `id` field is an identifier associated with the server.
#[derive(Clone)]
pub struct Server {
    pub(crate) clients: Arc<Mutex<HashMap<(usize, ChannelId), Client>>>,
    pub(crate) docker: Docker,
    pub(crate) id: usize,
}

/// Creates a closure that forwards the output of a container to a session channel.
///
/// # Arguments
///
/// * `channel` - The ID of the channel to send the output to.
/// * `cloned_handle` - A cloned handle to the session.
///
/// # Returns
///
/// A boxed closure that takes a `Result<LogOutput, Error>` as input and returns a `Pin<Box<dyn Future<Output = ()> + Send + 'static>>`.
///
/// #Example
/// ```
/// let output = [`Stream<Item=Result<LogOutput, Error>>`]
/// let session_handle = /* Create your session handle */;
/// let channel = /* Define your channel */;
///
///     output
///         .for_each(forward_container_output_to_session(channel, cloned_handle))
///         .await;
///```

fn forward_container_output_to_session(
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
    /// Create and start an exec process for a Docker container.
    ///
    /// # Arguments
    ///
    /// - `docker`: A reference to the Docker client.
    /// - `args`: The container arguments.
    /// - `container_id`: The ID of the container.
    ///
    /// # Returns
    ///
    /// A `Result` containing `StartExecResults` if the exec process is created and started successfully,
    /// or an `anyhow::Error` if an error occurred.
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
        if let StartExecResults::Attached { input, output } = process {
            self.link_io(channel, session_handle, client_id, input, output)
                .await;
        };
    }

    /// Establishes a link between an input stream and an output stream and the client's session.
    ///
    /// # Arguments
    ///
    /// * `channel` - The ID of the channel used for communication.
    /// * `session_handle` - The handle to the session.
    /// * `client_id` - The ID of the client.
    /// * `input` - The input stream to read from.
    /// * `output` - The output stream to write to.
    ///
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
            input,
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
                .for_each(forward_container_output_to_session(channel, cloned_handle))
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
