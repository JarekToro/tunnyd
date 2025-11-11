# Architecture and Design

## Overview

Tunnyd is a surrogate SSH daemon that enables secure, seamless access to Docker containers without exposing SSH ports or configuring complex network infrastructure. It acts as an intelligent proxy that bridges SSH connections to Docker containers based on custom hostname patterns.

## System Architecture

```
┌─────────────┐
│   Client    │
│  SSH User   │
└──────┬──────┘
       │
       │ SSH Connection (port 22)
       │ + ProxyJump
       ▼
┌──────────────────┐
│   Real Server    │
│   (Jump Host)    │
└──────┬───────────┘
       │
       │ SSH Connection (port 2222)
       │ RemoteCommand: tunnyd --target %n --user %r
       ▼
┌──────────────────────────────────────────┐
│            Tunnyd Process                │
│  ┌────────────────────────────────────┐  │
│  │   SSH Server (russh)               │  │
│  │   - Port 2222                      │  │
│  │   - No auth required               │  │
│  │   - Accepts connections            │  │
│  └───────────┬────────────────────────┘  │
│              │                            │
│              │ Parse CLI args             │
│              │ --target, --user           │
│              ▼                            │
│  ┌────────────────────────────────────┐  │
│  │   Container Lookup                 │  │
│  │   - Query Docker API               │  │
│  │   - Match labels                   │  │
│  │   - Validate user access           │  │
│  └───────────┬────────────────────────┘  │
│              │                            │
│              │ Found matching container   │
│              ▼                            │
│  ┌────────────────────────────────────┐  │
│  │   Docker Exec Session              │  │
│  │   - docker exec -it                │  │
│  │   - Start bash shell               │  │
│  │   - Stream I/O                     │  │
│  └───────────┬────────────────────────┘  │
│              │                            │
└──────────────┼────────────────────────────┘
               │
               │ I/O Streaming
               ▼
        ┌──────────────┐
        │   Docker     │
        │  Container   │
        └──────────────┘
```

## Core Components

### 1. Main Module (`main.rs`)

**Responsibility**: Application initialization and SSH server lifecycle management

**Key Operations**:
- Initializes async runtime with Tokio
- Establishes connection to Docker daemon via Unix socket
- Configures SSH server with russh library
- Generates ephemeral ED25519 key pair for SSH
- Runs SSH server on `0.0.0.0:2222` with automatic retry on failure

**Configuration**:
```rust
- Inactivity timeout: 3600 seconds (1 hour)
- Auth rejection time: 3 seconds
- Auth methods: None (accepts all connections)
- Port: 2222
```

**Error Handling**: Implements retry loop for server failures, ensuring service resilience.

### 2. CLI Module (`cli.rs`)

**Responsibility**: Parses SSH command arguments to extract target hostname and username

**Data Structures**:
- `ContainerArgs`: Holds `target` (required) and `user` (optional)

**Key Functions**:
- `parse_and_match_args(&[u8]) -> ContainerArgs`
  - Uses `shlex` for shell-style parsing
  - Leverages `clap` for argument validation
  - Expected format: `--target <hostname> [--user <username>]`

**Example**:
```bash
# SSH command sends this to Tunnyd:
tunnyd --target my-app.my-docker --user git
# Parsed as: ContainerArgs { target: "my-app.my-docker", user: Some("git") }
```

### 3. Docker Module (`docker.rs`)

**Responsibility**: Docker API integration and container discovery

**Key Functions**:

#### `connect_to_docker() -> Result<Docker, Error>`
- Connects to Docker daemon using local defaults (typically `/var/run/docker.sock`)
- Returns reusable `Docker` client instance

#### `find_ssh_enabled_container(args: &ContainerArgs) -> Result<ContainerSummary, Error>`
- Lists all containers (including stopped ones)
- Filters by label-based matching rules
- Returns first matching container or error

#### `check_container_validity(labels, target, user) -> bool`
- Validates three conditions:
  1. `tunnyD.enable=true` (container is SSH-enabled)
  2. `tunnyD.hostname=<target>` (matches requested hostname)
  3. `tunnyD.allowed.users` is empty OR contains `<user>` (user is authorized)

**Label Schema**:
```yaml
labels:
  - tunnyD.enable=true              # Required: enable SSH access
  - tunnyD.hostname=app.my-docker   # Required: hostname pattern
  - tunnyD.allowed.users=user1,user2 # Optional: comma-separated user list
```

### 4. Server Module (`server.rs`)

**Responsibility**: SSH protocol handling and Docker exec session management

**Data Structures**:

#### `Server`
```rust
pub struct Server {
    clients: Arc<Mutex<HashMap<(usize, ChannelId), Client>>>,
    docker: Docker,
    id: usize,
}
```
- `clients`: Thread-safe map of active SSH sessions
- `docker`: Shared Docker API client
- `id`: Incremental client identifier

#### `Client`
```rust
pub struct Client {
    session_handle: russh::server::Handle,
    io: Option<OutputInputPair>,
}
```
- `session_handle`: SSH session control handle
- `io`: Bidirectional streams for container I/O

#### `OutputInputPair`
```rust
pub struct OutputInputPair {
    output: Arc<Mutex<Stream<Result<LogOutput, Error>>>>,
    input: Pin<Box<dyn AsyncWrite + Send>>,
}
```
- `output`: Stream of container stdout/stderr
- `input`: Sink for SSH input to container stdin

**SSH Handler Implementation**:

The `Server` implements `russh::server::Handler` trait with these key methods:

| Method | Purpose | Implementation |
|--------|---------|----------------|
| `channel_open_session` | New SSH channel created | Registers client in hashmap |
| `exec_request` | Client sends command | Parses args, finds container, starts exec |
| `data` | Client sends input data | Writes to container stdin |
| `auth_publickey` | Public key auth | Always accepts (no auth required) |
| `auth_none` | No-auth method | Always accepts |
| `channel_close` | Session termination | Cleanup (currently minimal) |

**Exec Session Flow**:

1. **Create Exec Session** (`create_and_start_exec`):
   ```rust
   CreateExecOptions {
       attach_stdout: true,
       attach_stderr: true,
       attach_stdin: true,
       cmd: vec!["bash"],      // Interactive bash shell
       tty: true,               // Allocate pseudo-TTY
       user: args.user,         // Run as specified user
   }
   ```

2. **Link I/O Streams** (`link_io`):
   - Stores `OutputInputPair` in client record
   - Spawns async task to forward container output to SSH channel
   - Uses `forward_container_output_to_session` closure for streaming

3. **Bidirectional Communication**:
   - **Client → Container**: `data()` handler writes to `io.input`
   - **Container → Client**: Background task reads from `io.output` stream
   - **Termination**: Sends "Docker Container exited process" message on stream end

## Data Flow

### Connection Establishment

```
1. User runs: ssh git@my-app.my-docker
   ↓
2. SSH config matches *.my-docker pattern
   ↓
3. SSH client connects to jump host (ProxyJump)
   ↓
4. SSH client connects to 192.168.100.100:2222
   ↓
5. SSH client sends RemoteCommand: tunnyd --target my-app.my-docker --user git
   ↓
6. Tunnyd SSH server accepts connection (no auth)
   ↓
7. Server.exec_request() is called with command data
   ↓
8. CLI module parses: ContainerArgs { target: "my-app.my-docker", user: Some("git") }
   ↓
9. Docker module finds container with matching labels:
   - tunnyD.enable=true
   - tunnyD.hostname=my-app.my-docker
   - tunnyD.allowed.users contains "git"
   ↓
10. Server creates docker exec session with bash shell
    ↓
11. I/O streams are linked:
    - SSH input → container stdin
    - Container stdout/stderr → SSH output
    ↓
12. User has interactive shell in container
```

### Data Streaming

```
┌──────────────────────────────────────────────────┐
│  SSH Client Input (keystrokes)                   │
└───────────────────┬──────────────────────────────┘
                    │
                    ▼
┌──────────────────────────────────────────────────┐
│  Server.data() handler                           │
│  - Receives bytes from SSH channel               │
│  - Locks client's IO pair                        │
│  - Writes to io.input (AsyncWrite)               │
└───────────────────┬──────────────────────────────┘
                    │
                    ▼
┌──────────────────────────────────────────────────┐
│  Docker Exec stdin (container process)           │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  Docker Exec stdout/stderr (container output)    │
└───────────────────┬──────────────────────────────┘
                    │
                    ▼
┌──────────────────────────────────────────────────┐
│  io.output stream (async task)                   │
│  - Tokio task spawned in link_io()               │
│  - Reads LogOutput from stream                   │
│  - Calls forward_container_output_to_session()   │
└───────────────────┬──────────────────────────────┘
                    │
                    ▼
┌──────────────────────────────────────────────────┐
│  session_handle.data(channel, bytes)             │
│  - Sends output to SSH channel                   │
│  - Client sees output in terminal                │
└──────────────────────────────────────────────────┘
```

## Security Model

### Authentication Strategy

**Intentional No-Auth Design**: Tunnyd does not perform authentication itself. Security is provided by:

1. **ProxyJump Barrier**: Users must first authenticate to the jump host
2. **Network Isolation**: Tunnyd listens on `0.0.0.0:2222` but should be firewalled to local access only
3. **Container-Level Access Control**: `tunnyD.allowed.users` label restricts which users can access specific containers

### Security Considerations

**Current Implementation**:
- `auth_publickey()`: Always returns `Auth::Accept`
- `auth_none()`: Always returns `Auth::Accept`
- Comments indicate: "Purposely left this way, don't change or refactor"

**Threat Model**:
- Tunnyd assumes it's running in a trusted environment
- Port 2222 should NOT be exposed to the internet
- Users must pass through SSH authentication on the jump host
- Container access control is enforced via Docker labels

**Recommendations for Production**:
- Use firewall rules to restrict port 2222 to localhost
- Implement audit logging for all container access
- Consider adding mTLS for additional security layers
- Use Docker socket over TLS if Docker daemon is remote

## Concurrency and State Management

### Thread Safety

**Shared State**:
```rust
Arc<Mutex<HashMap<(usize, ChannelId), Client>>>
```
- `Arc`: Enables shared ownership across async tasks
- `Mutex`: Provides exclusive access for mutations
- Key: `(client_id, channel_id)` tuple for unique session identification

**Async Runtime**:
- Uses Tokio for async I/O and task scheduling
- Each client connection handled in separate task context
- Background tasks spawned for output streaming

### Server Lifecycle

```
┌─────────────────────────────────────────┐
│  Main Loop                              │
│  ┌───────────────────────────────────┐  │
│  │  Spawn server task                │  │
│  │  Listen on port 2222              │  │
│  │  ↓                                 │  │
│  │  Error? → Retry                   │  │
│  │  Success? → Run until terminated  │  │
│  └───────────────────────────────────┘  │
│                                         │
│  Uses mpsc channel for error reporting │
│  Infinite retry on server crash        │
└─────────────────────────────────────────┘
```

## Error Handling

### Docker Errors
- Connection failure: Panics with "Docker connection failed"
- Container not found: Returns `Error::DockerContainerWaitError`
- Exec creation failure: Logs error and returns `anyhow::Error`

### SSH Errors
- Client not found: Returns `anyhow::Error::msg("Client Not ready")`
- Write failures: Silently ignored with `map_or((), |_| ())`

### Resilience
- Server restart loop ensures Tunnyd stays running
- Graceful handling of container exit
- Auto-cleanup of streams on session end

## Performance Characteristics

### Resource Usage
- Minimal overhead per connection (async I/O)
- Shared Docker client across all sessions
- No buffering of container output (streaming)

### Scalability
- Concurrent clients: Limited by Tokio thread pool
- Docker daemon: Handles exec sessions natively
- Memory: O(n) where n = active sessions

## Future Considerations

### Potential Enhancements
1. **Audit Logging**: Track all container access attempts
2. **Metrics**: Prometheus exporter for connection stats
3. **Health Checks**: Readiness/liveness endpoints
4. **Session Recording**: Optional session replay capability
5. **Rate Limiting**: Per-user connection limits
6. **Dynamic Configuration**: Reload rules without restart
7. **Multi-Docker Host**: Support multiple Docker daemons

### Known Limitations
1. No support for SSH port forwarding
2. No session persistence across Tunnyd restarts
3. Single Docker daemon connection only
4. No connection timeout enforcement
5. Limited error recovery options

## Deployment Architecture

### Recommended Setup

```
Internet
    ↓
[Firewall/NAT]
    ↓
[Jump Host - Port 22]
    ↓ (local only)
[Docker Host - Port 2222 - Tunnyd]
    ↓ (unix socket)
[Docker Daemon - /var/run/docker.sock]
    ↓
[Containers with tunnyD labels]
```

### System Requirements
- Linux host with Docker installed
- Docker socket accessible at `/var/run/docker.sock`
- Port 2222 available (configurable in code)
- Rust 2021 edition or later for compilation

## Configuration Management

### Compile-Time Configuration
All settings are currently hardcoded in source:
- Port: `main.rs:48` (`2222`)
- Timeouts: `main.rs:24-26`
- Shell command: `server.rs:161` (`bash`)

### Runtime Configuration
Provided via Docker labels on containers:
- `tunnyD.enable`
- `tunnyD.hostname`
- `tunnyD.allowed.users`

### Environment Variables
Uses `SSH_ORIGINAL_COMMAND` (referenced but unused in current version)

---

This architecture provides a secure, efficient, and maintainable solution for Docker container access via SSH, with clear separation of concerns and robust error handling.
