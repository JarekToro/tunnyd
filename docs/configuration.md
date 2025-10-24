# Configuration Reference

This document provides comprehensive reference for configuring Tunnyd and Docker containers for SSH access.

## Table of Contents

- [Docker Container Labels](#docker-container-labels)
- [SSH Configuration](#ssh-configuration)
- [CLI Arguments](#cli-arguments)
- [Server Configuration](#server-configuration)
- [Environment Variables](#environment-variables)
- [Examples](#examples)

## Docker Container Labels

Tunnyd uses Docker container labels to control access and routing. All labels use the `tunnyD.` prefix.

### Required Labels

#### `tunnyD.enable`

**Type**: Boolean string (`"true"` or `"false"`)
**Required**: Yes
**Default**: N/A

Enables SSH access to the container via Tunnyd. If not set to `"true"`, the container will be ignored.

**Examples**:
```yaml
labels:
  - tunnyD.enable=true   # Enable SSH access
```

#### `tunnyD.hostname`

**Type**: String
**Required**: Yes
**Default**: N/A

The hostname pattern that SSH clients use to target this container. This value is matched against the `--target` CLI argument.

**Format**: Any string that matches your SSH config pattern

**Best Practices**:
- Use descriptive, memorable names
- Follow a consistent naming convention
- Include environment identifiers if needed (e.g., `app-prod`, `app-dev`)
- Match the pattern in your SSH config file (e.g., `*.my-docker`)

**Examples**:
```yaml
# Simple hostname
labels:
  - tunnyD.hostname=my-app.my-docker

# Environment-specific
labels:
  - tunnyD.hostname=postgres-prod.my-docker

# Service-based naming
labels:
  - tunnyD.hostname=api-gateway.my-docker
```

### Optional Labels

#### `tunnyD.allowed.users`

**Type**: Comma-separated string list
**Required**: No
**Default**: Empty (allows all users)

Restricts which SSH users can access the container. If empty or not set, all users are allowed. If set, only specified users can access the container.

**Format**: `user1,user2,user3` (no spaces)

**Behavior**:
- Empty or missing: All users allowed
- Populated: Only listed users allowed
- User matching is case-sensitive
- Whitespace is significant (avoid spaces)

**Examples**:
```yaml
# Allow specific users
labels:
  - tunnyD.allowed.users=git,deploy,admin

# Allow single user
labels:
  - tunnyD.allowed.users=root

# Allow all users (omit label or leave empty)
labels:
  - tunnyD.allowed.users=
```

### Complete Label Examples

#### Public Access Container
```yaml
services:
  public-app:
    image: ubuntu:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=public.my-docker
      # No allowed.users - anyone can connect
```

#### Restricted Access Container
```yaml
services:
  production-db:
    image: postgres:15
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=db-prod.my-docker
      - tunnyD.allowed.users=dba,admin,backup
```

#### Development Container
```yaml
services:
  dev-workspace:
    image: node:20
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=dev.my-docker
      - tunnyD.allowed.users=developer,devops
```

## SSH Configuration

### Client SSH Config (`~/.ssh/config`)

Tunnyd requires specific SSH client configuration to work correctly. Add this to your `~/.ssh/config` file:

```ssh-config
Host *.my-docker
    HostName 192.168.100.100
    Port 2222
    PreferredAuthentications none
    RequestTTY yes
    ProxyJump user@jumphost
    RemoteCommand tunnyd --target %n --user %r
```

### Configuration Breakdown

#### `Host *.my-docker`
**Purpose**: Pattern matching for container hostnames

**Details**:
- Matches any hostname ending with `.my-docker`
- Examples: `app.my-docker`, `db.my-docker`, `anything.my-docker`
- Change `.my-docker` to your preferred domain pattern

#### `HostName 192.168.100.100`
**Purpose**: IP address of the server running Tunnyd

**Details**:
- Replace with your Docker host's IP address
- Can also use a hostname if DNS is configured
- This is the server where Docker containers are running

#### `Port 2222`
**Purpose**: Tunnyd SSH service port

**Details**:
- Default Tunnyd port is `2222`
- Must match the port Tunnyd is listening on
- If you modify the port in `main.rs`, update this value

#### `PreferredAuthentications none`
**Purpose**: Disable SSH authentication to Tunnyd

**Details**:
- Tunnyd does not require authentication (handled by ProxyJump)
- Security is enforced at the jump host level
- Speeds up connection establishment

#### `RequestTTY yes`
**Purpose**: Allocate pseudo-terminal for interactive sessions

**Details**:
- Enables interactive shell functionality
- Required for proper terminal emulation
- Allows running interactive programs (vim, top, etc.)

#### `ProxyJump user@jumphost`
**Purpose**: Security layer and connection routing

**Details**:
- Replace `user` with your SSH username
- Replace `jumphost` with your jump host address
- All connections go through this authenticated hop
- Provides security boundary

**Example variations**:
```ssh-config
# With specific port
ProxyJump user@jumphost:2222

# With SSH key
ProxyJump -i ~/.ssh/id_rsa user@jumphost

# Multiple jumps
ProxyJump user@bastion1,user@bastion2
```

#### `RemoteCommand tunnyd --target %n --user %r`
**Purpose**: Execute Tunnyd with parsed arguments

**Details**:
- `%n`: Expands to the hostname used by SSH client
- `%r`: Expands to the remote username
- Tunnyd parses these arguments to find the correct container

**How it works**:
```bash
# User runs:
ssh git@my-app.my-docker

# SSH client expands to:
tunnyd --target my-app.my-docker --user git
```

### Advanced SSH Configuration

#### Connection Multiplexing
```ssh-config
Host *.my-docker
    ControlMaster auto
    ControlPath ~/.ssh/sockets/%r@%h:%p
    ControlPersist 10m
```
Speeds up subsequent connections by reusing existing connections.

#### Connection Timeout
```ssh-config
Host *.my-docker
    ConnectTimeout 10
    ServerAliveInterval 60
    ServerAliveCountMax 3
```
Prevents hanging connections and keeps sessions alive.

#### Logging
```ssh-config
Host *.my-docker
    LogLevel DEBUG3
```
Useful for troubleshooting connection issues.

### Complete SSH Config Example

```ssh-config
# Tunnyd Container Access
Host *.my-docker
    # Target server
    HostName 192.168.100.100
    Port 2222

    # Security
    PreferredAuthentications none
    ProxyJump myuser@bastion.example.com

    # Session settings
    RequestTTY yes
    RemoteCommand tunnyd --target %n --user %r

    # Connection optimization
    ControlMaster auto
    ControlPath ~/.ssh/sockets/%r@%h:%p
    ControlPersist 10m

    # Timeouts
    ConnectTimeout 10
    ServerAliveInterval 60
    ServerAliveCountMax 3

    # Logging (optional, for debugging)
    # LogLevel DEBUG

# Jump host configuration
Host bastion.example.com
    User myuser
    IdentityFile ~/.ssh/id_rsa
    Port 22
```

## CLI Arguments

Tunnyd accepts command-line arguments passed via SSH RemoteCommand.

### `--target` / `-t`

**Type**: String
**Required**: Yes
**Description**: The hostname that relates to the Docker container

**Usage**:
```bash
tunnyd --target my-app.my-docker
tunnyd -t my-app.my-docker
```

**Matching Logic**:
- Must exactly match a container's `tunnyD.hostname` label
- Case-sensitive matching
- Full hostname must match (no partial matching)

### `--user` / `-u`

**Type**: String
**Required**: No
**Default**: Empty string
**Description**: The user to use to login to the Docker container

**Usage**:
```bash
tunnyd --target my-app.my-docker --user git
tunnyd -t my-app.my-docker -u root
```

**Behavior**:
- If provided, checks against `tunnyD.allowed.users` label
- If not provided, empty string used for validation
- Empty `allowed.users` label allows any user (including no user)

**Docker Exec Context**:
- Passed to `docker exec -u <user>`
- Determines which user the shell runs as inside container
- Must exist in container's user database

## Server Configuration

Server configuration is currently compile-time only, defined in source code.

### SSH Server Settings

**Location**: `src/main.rs:23-30`

```rust
russh::server::Config {
    inactivity_timeout: Some(Duration::from_secs(3600)),
    auth_rejection_time: Duration::from_secs(3),
    auth_rejection_time_initial: Some(Duration::from_secs(10)),
    keys: vec![KeyPair::generate_ed25519().unwrap()],
    methods: MethodSet::NONE,
    ..Default::default()
}
```

#### `inactivity_timeout`
**Default**: `3600` seconds (1 hour)
**Purpose**: Maximum idle time before disconnection
**Modification**: Change `Duration::from_secs(3600)` value

#### `auth_rejection_time`
**Default**: `3` seconds
**Purpose**: Delay before rejecting authentication (not used)
**Modification**: Change `Duration::from_secs(3)` value

#### `auth_rejection_time_initial`
**Default**: `10` seconds
**Purpose**: Initial delay before rejecting authentication (not used)
**Modification**: Change `Duration::from_secs(10)` value

#### `keys`
**Default**: Ephemeral ED25519 key pair
**Purpose**: SSH host key (regenerated on each start)
**Modification**: Load persistent key from file if needed

#### `methods`
**Default**: `MethodSet::NONE`
**Purpose**: Disables authentication requirements
**Modification**: Not recommended (security by ProxyJump)

### Listening Address

**Location**: `src/main.rs:48`

```rust
russh::server::run(config_clone, ("0.0.0.0", 2222), server_clone)
```

#### Bind Address
**Default**: `0.0.0.0` (all interfaces)
**Recommendations**:
- Production: `127.0.0.1` (localhost only)
- Development: `0.0.0.0` (all interfaces)
- Docker network: Container IP

#### Port
**Default**: `2222`
**Modification**: Change second tuple element
**Note**: Must match SSH config `Port` directive

### Logging Configuration

**Location**: `src/main.rs:16-18`

```rust
env_logger::builder()
    .filter_level(log::LevelFilter::Warn)
    .init();
```

#### Log Level
**Default**: `Warn`
**Options**: `Error`, `Warn`, `Info`, `Debug`, `Trace`
**Environment Override**: Set `RUST_LOG` environment variable

**Examples**:
```bash
# Show all logs
RUST_LOG=debug ./tunnyd

# Show info and above
RUST_LOG=info ./tunnyd

# Module-specific logging
RUST_LOG=tunnyd=debug,bollard=warn ./tunnyd
```

### Docker Connection

**Location**: `src/docker.rs:183-191`

```rust
Docker::connect_with_local_defaults()
```

**Default Behavior**:
- Connects to `/var/run/docker.sock` (Unix)
- Uses environment variables if set
- Respects `DOCKER_HOST`, `DOCKER_CERT_PATH`, etc.

**Alternative Connections**:
```rust
// TCP connection
Docker::connect_with_http("tcp://127.0.0.1:2375", 120, API_DEFAULT_VERSION)

// TLS connection
Docker::connect_with_tls("tcp://127.0.0.1:2376", &cert_path, 120, API_DEFAULT_VERSION)
```

### Docker Exec Settings

**Location**: `src/server.rs:157-165`

```rust
CreateExecOptions {
    attach_stdout: Some(true),
    attach_stderr: Some(true),
    attach_stdin: Some(true),
    cmd: Some(vec!["bash"]),
    tty: Some(true),
    user: args.user.as_ref().map(|s| s.as_str()),
    ..Default::default()
}
```

#### Shell Command
**Default**: `bash`
**Modification**: Change `vec!["bash"]` to another shell
**Alternatives**: `"sh"`, `"zsh"`, `"fish"`, `"/bin/bash"`

**Considerations**:
- Shell must exist in container
- Consider using `sh` for broader compatibility
- Can pass shell options: `vec!["bash", "-l"]` for login shell

## Environment Variables

### `RUST_LOG`
**Purpose**: Controls logging verbosity
**Format**: `module=level,module=level`
**Examples**:
```bash
RUST_LOG=debug
RUST_LOG=tunnyd=trace,russh=debug
RUST_LOG=warn
```

### Docker Environment Variables

Tunnyd respects standard Docker environment variables:

- `DOCKER_HOST`: Docker daemon address
- `DOCKER_CERT_PATH`: TLS certificate path
- `DOCKER_TLS_VERIFY`: Enable TLS verification
- `DOCKER_API_VERSION`: Docker API version to use

**Example**:
```bash
export DOCKER_HOST=tcp://192.168.1.100:2376
export DOCKER_TLS_VERIFY=1
export DOCKER_CERT_PATH=/path/to/certs
./tunnyd
```

## Examples

### Example 1: Basic Single Container

**Docker Compose**:
```yaml
version: "3.8"
services:
  web:
    image: nginx:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=web.my-docker
```

**SSH Config**:
```ssh-config
Host *.my-docker
    HostName 192.168.1.100
    Port 2222
    PreferredAuthentications none
    RequestTTY yes
    ProxyJump user@bastion
    RemoteCommand tunnyd --target %n --user %r
```

**Usage**:
```bash
ssh root@web.my-docker
```

### Example 2: Multiple Containers with Access Control

**Docker Compose**:
```yaml
version: "3.8"
services:
  database:
    image: postgres:15
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=db.my-docker
      - tunnyD.allowed.users=dba,backup

  api:
    image: node:20
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=api.my-docker
      - tunnyD.allowed.users=developer,deploy

  frontend:
    image: nginx:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=web.my-docker
      # No user restrictions
```

**Usage**:
```bash
# DBA accessing database
ssh dba@db.my-docker

# Developer accessing API
ssh developer@api.my-docker

# Anyone accessing frontend
ssh anyuser@web.my-docker
```

### Example 3: Environment-Based Naming

**Docker Compose** (Production):
```yaml
version: "3.8"
services:
  app:
    image: myapp:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=app-prod.my-docker
      - tunnyD.allowed.users=ops,deploy
```

**Docker Compose** (Staging):
```yaml
version: "3.8"
services:
  app:
    image: myapp:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=app-staging.my-docker
      - tunnyD.allowed.users=developer,qa,ops
```

**Usage**:
```bash
# Production access (restricted)
ssh ops@app-prod.my-docker

# Staging access (more permissive)
ssh developer@app-staging.my-docker
```

### Example 4: Service-Oriented Architecture

**Docker Compose**:
```yaml
version: "3.8"
services:
  auth-service:
    image: auth:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=auth.svc.my-docker
      - tunnyD.allowed.users=backend-dev

  payment-service:
    image: payment:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=payment.svc.my-docker
      - tunnyD.allowed.users=payment-team,backend-dev

  notification-service:
    image: notifications:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=notify.svc.my-docker
```

**Usage**:
```bash
ssh backend-dev@auth.svc.my-docker
ssh payment-team@payment.svc.my-docker
ssh anyone@notify.svc.my-docker
```

### Example 5: Custom Shell Configuration

**Dockerfile**:
```dockerfile
FROM ubuntu:22.04

# Install zsh
RUN apt-get update && apt-get install -y zsh

# Set zsh as default shell
RUN chsh -s /bin/zsh

LABEL tunnyD.enable="true"
LABEL tunnyD.hostname="custom.my-docker"
```

**Modification in `server.rs:161`**:
```rust
cmd: Some(vec!["zsh"]),  // Changed from "bash"
```

## Troubleshooting

### Container Not Found

**Symptom**: "No Available Container matches" error

**Checks**:
1. Verify `tunnyD.enable=true` label exists
2. Verify `tunnyD.hostname` matches `--target` argument exactly
3. Check `tunnyD.allowed.users` includes the user (if set)
4. Ensure container is running: `docker ps`
5. Verify labels: `docker inspect <container> | grep tunnyD`

### Connection Timeout

**Symptom**: SSH connection hangs or times out

**Checks**:
1. Verify Tunnyd is running: `ps aux | grep tunnyd`
2. Check Tunnyd is listening: `netstat -tlnp | grep 2222`
3. Verify firewall allows port 2222
4. Check ProxyJump host is accessible
5. Review SSH config for typos

### Permission Denied

**Symptom**: User cannot access container

**Checks**:
1. Verify user in `tunnyD.allowed.users` label (if set)
2. Check for typos in username (case-sensitive)
3. Ensure user exists in container: `docker exec <container> id <user>`
4. Check Docker socket permissions

### Shell Not Found

**Symptom**: "bash: not found" or similar error

**Solution**:
- Change shell in `server.rs:161` to `"sh"`
- Install bash in container
- Use alternative shell available in container

---

For more information, see:
- [Architecture Documentation](architecture.md)
- [API/Module Documentation](api.md)
- [README](../README.md)
