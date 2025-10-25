<img src=".github/assets/tunnyd.svg" width="200px" height="200px"/>

# Tunnyd

[![Rust](https://github.com/JarekToro/tunnyd/actions/workflows/rust.yml/badge.svg)](https://github.com/JarekToro/tunnyd/actions/workflows/rust.yml)

**SSH directly into your Docker containers without exposing ports or managing complex networking.**

Tunnyd is a lightweight SSH proxy that lets you access Docker containers by hostname through your existing SSH infrastructure. Label your containers, configure your SSH client once, and connect to any container with a simple `ssh` command.

## The Problem

Running SSH daemons in Docker containers is cumbersome:
- Each container needs its own exposed SSH port (2222, 2223, 2224...)
- You have to manage SSH keys and users inside every container
- Port management becomes a nightmare with many containers
- Bastion host configurations are complex and error-prone
- Container images bloat with SSH server installations

**There has to be a better way.**

## How Tunnyd Solves It

Tunnyd acts as a smart SSH proxy on your Docker host:

1. **No SSH in containers** - Containers don't need SSH servers installed
2. **No port exposure** - Tunnyd uses `docker exec` internally, no container ports needed
3. **Hostname-based routing** - Access containers by name: `ssh user@myapp.docker`
4. **Label-based access control** - Simple Docker labels control who can access what
5. **Works with existing SSH** - Uses your SSH client, keys, and ProxyJump setup

**One Tunnyd instance serves all your containers.**

## Key Features

- **Zero container modifications** - Use any image, no SSH installation needed
- **Dynamic discovery** - Containers are found automatically via Docker labels
- **Access control** - Restrict users per container with simple labels
- **Secure by design** - Leverages SSH ProxyJump for authentication
- **Minimal overhead** - Written in Rust for performance and safety
- **Simple setup** - Configure once, works for all containers

## Quick Start

### Prerequisites

- Docker installed and running
- Rust toolchain (for building from source)
- SSH access to your Docker host (for ProxyJump)
- Docker socket access at `/var/run/docker.sock`

### Installation

1. **Clone and build:**
```bash
git clone https://github.com/yourusername/tunnyd.git
cd tunnyd
cargo build --release
```

2. **Copy binary to your Docker host:**
```bash
# On your Docker host
sudo cp target/release/main /usr/local/bin/tunnyd
sudo chmod +x /usr/local/bin/tunnyd
```

3. **Run Tunnyd** (on your Docker host):
```bash
tunnyd
# Listens on port 2222
```

### Basic Configuration

**Step 1: Label your Docker containers**

Add these labels to containers you want to access via SSH:

```yaml
version: "3.8"
services:
  myapp:
    image: ubuntu:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=myapp.docker
      - tunnyD.allowed.users=developer,admin  # Optional: restrict users
```

**Step 2: Configure your SSH client** (`~/.ssh/config`):

```ssh
Host *.docker
    HostName 192.168.1.100       # Your Docker host IP
    Port 2222                    # Tunnyd listens on this port
    User %r                      # Pass through your username
    ProxyJump user@192.168.1.100 # SSH to SAME host port 22 first (authenticate)
    PreferredAuthentications none
    RequestTTY yes
    RemoteCommand tunnyd --target %n --user %r
```

**Important:** `ProxyJump` and `HostName` are the **same server**. You SSH to the host on port 22 (authenticate), then connect to port 2222 (Tunnyd) on the same machine.

**Step 3: Connect!**

```bash
ssh developer@myapp.docker
# You're now in a bash shell inside the container!
```

## How It Works

Here's what happens when you run `ssh developer@myapp.docker`:

```
1. Your SSH Client
   ├─> Connects to Docker host on port 22 (ProxyJump)
   │   └─> Standard SSH daemon authenticates you with SSH key
   │   └─> ✓ Authentication happens HERE
   │
2. From Docker Host → Back to Docker Host on port 2222
   ├─> ProxyJump forwards you to Tunnyd (same server, different port)
   ├─> Sends RemoteCommand: "tunnyd --target myapp.docker --user developer"
   │
3. Tunnyd (port 2222)
   ├─> NO authentication (you already proved you can SSH to the host)
   ├─> Queries Docker API for containers
   ├─> Finds container with labels:
   │     • tunnyD.enable=true
   │     • tunnyD.hostname=myapp.docker
   │     • tunnyD.allowed.users contains "developer"
   │
4. Tunnyd → Docker
   ├─> Runs: docker exec -it -u developer <container> bash
   │
5. Tunnyd bridges I/O
   ├─> Your keystrokes → container stdin
   └─> Container stdout/stderr → your terminal

✓ You now have an interactive shell in the container
```

### The Key Architectural Detail

**ProxyJump connects to the SAME server twice on different ports:**

```
┌─────────────────────────────────────┐
│    Docker Host (192.168.1.100)      │
│                                     │
│  Port 22              Port 2222     │
│  ┌──────────┐        ┌──────────┐  │
│  │   SSH    │        │ Tunnyd   │  │
│  │  Daemon  │───────>│ (proxy)  │  │
│  │ (auth)   │  jump  │ (no auth)│  │
│  └──────────┘        └─────┬────┘  │
│       ▲                    │        │
│       │                    │        │
│       │                    ▼        │
│   You connect          docker exec  │
│   here first           to container │
│                                     │
└─────────────────────────────────────┘
```

**Why this architecture?**
- **Port 22**: Regular SSH daemon handles authentication (SSH keys, passwords)
- **Port 2222**: Tunnyd just routes to containers (no auth needed - you're already in!)
- **Security**: If you can SSH to the host, you can access Tunnyd. Simple.

**Security Note:** Tunnyd itself does not authenticate users. Security is enforced by the host's SSH daemon on port 22. Only users who can SSH to the Docker host can access Tunnyd on port 2222.

## Configuration

### Docker Container Labels

Three labels control container access:

| Label | Required | Description | Example |
|-------|----------|-------------|---------|
| `tunnyD.enable` | **Yes** | Must be `"true"` to enable access | `tunnyD.enable=true` |
| `tunnyD.hostname` | **Yes** | Hostname for SSH access | `tunnyD.hostname=api.docker` |
| `tunnyD.allowed.users` | No | Comma-separated users (empty = all allowed) | `tunnyD.allowed.users=git,deploy` |

### Docker Compose Example

```yaml
version: "3.8"
services:
  # Public access container
  web:
    image: nginx:latest
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=web.docker
      # No allowed.users = anyone can access

  # Restricted access container
  database:
    image: postgres:15
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=db.docker
      - tunnyD.allowed.users=dba,admin

  # Developer workspace
  devbox:
    image: node:20
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=dev.docker
      - tunnyD.allowed.users=alice,bob,carol
```

### SSH Client Configuration

Edit `~/.ssh/config`:

```ssh
Host *.docker
    # Docker host running Tunnyd
    HostName 192.168.1.100
    Port 2222

    # Security: First SSH to the SAME host on port 22 (authenticate there)
    ProxyJump myuser@192.168.1.100

    # Pass through username
    User %r

    # Tunnyd doesn't authenticate (handled by ProxyJump on port 22)
    PreferredAuthentications none

    # Enable interactive terminal
    RequestTTY yes

    # Execute Tunnyd with target and user
    RemoteCommand tunnyd --target %n --user %r
```

**Critical Detail:** `ProxyJump` connects to the **same IP address** as `HostName`:
- First: SSH to `192.168.1.100:22` (standard SSH, authenticate with your key)
- Then: From there, connect to `192.168.1.100:2222` (Tunnyd, no auth needed)

**Replace `192.168.1.100` with your actual Docker host IP in BOTH places.**

### Advanced SSH Configuration

**Connection multiplexing** (faster reconnections):
```ssh
Host *.docker
    ControlMaster auto
    ControlPath ~/.ssh/sockets/%r@%h:%p
    ControlPersist 10m
```

**Timeouts and keepalives:**
```ssh
Host *.docker
    ConnectTimeout 10
    ServerAliveInterval 60
    ServerAliveCountMax 3
```

## Usage Examples

### Basic Connection

```bash
# Connect as "developer" user
ssh developer@myapp.docker

# Connect as "root" user
ssh root@myapp.docker

# Run a single command
ssh developer@myapp.docker ls -la /var/log

# Run interactive command
ssh developer@myapp.docker -t htop
```

### Multiple Environments

```yaml
# docker-compose.yml
services:
  app-prod:
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=app-prod.docker
      - tunnyD.allowed.users=ops

  app-staging:
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=app-staging.docker
      - tunnyD.allowed.users=developer,qa
```

```bash
# Operations access to production
ssh ops@app-prod.docker

# Developer access to staging
ssh developer@app-staging.docker
```

### Service-Oriented Architecture

```yaml
services:
  auth-service:
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=auth.services.docker

  payment-service:
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=payment.services.docker

  notification-service:
    labels:
      - tunnyD.enable=true
      - tunnyD.hostname=notify.services.docker
```

```bash
ssh admin@auth.services.docker
ssh admin@payment.services.docker
ssh admin@notify.services.docker
```

## Troubleshooting

### Container not found

```bash
# Check container is running
docker ps | grep myapp

# Verify labels are set correctly
docker inspect myapp | grep tunnyD

# Expected output:
# "tunnyD.enable": "true",
# "tunnyD.hostname": "myapp.docker",
```

### Connection hangs

- Verify Tunnyd is running: `ps aux | grep tunnyd`
- Check Tunnyd is listening: `netstat -tlnp | grep 2222`
- Test ProxyJump host: `ssh myuser@dockerhost.example.com`
- Check firewall allows port 2222

### Permission denied

- Verify user in `tunnyD.allowed.users` label (if set)
- Check username matches exactly (case-sensitive)
- Ensure user exists in container: `docker exec myapp id developer`

### Shell not found

If you see "bash: not found", the container doesn't have bash installed.

**Option 1:** Change shell in Tunnyd source (`src/server.rs:161`):
```rust
cmd: Some(vec!["sh"]),  // Use sh instead of bash
```

**Option 2:** Install bash in your container image.

## Use Cases

**Perfect for:**
- Development environments with many microservices
- DevOps teams managing containerized infrastructure
- Debugging production containers without installing SSH
- Multi-tenant container platforms
- CI/CD environments needing temporary container access

**Not recommended for:**
- Production SSH access (use proper bastion hosts)
- Public-facing services (Tunnyd should be firewalled)
- Containers that need persistent SSH sessions

## Documentation

For detailed technical information:

- **[Architecture and Design](docs/architecture.md)** - System internals, data flow, and component details
- **[Configuration Reference](docs/configuration.md)** - Complete reference for all configuration options

## Contributing

Contributions are welcome! We appreciate:

- Bug reports and feature requests (open an issue)
- Documentation improvements
- Code contributions (open a pull request)
- Use case examples and tutorials

Please ensure your code follows Rust best practices and includes appropriate tests.

## License

Tunnyd is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

**Made with ❤️ in Rust**
