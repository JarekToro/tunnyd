
# Tunnyd: Easy Docker Container Tunneling with SSH

Tunnyd is a Rust program designed to simplify the process of accessing Docker containers remotely without the need for exposing SSH ports or configuring bastion hosts. By leveraging a custom name pattern, Tunnyd establishes a secure tunneling mechanism, enabling seamless access to Docker containers over SSH.

## Features

- **Dynamic Tunneling**: Tunnyd dynamically matches container labels with the provided custom name pattern, facilitating easy access to Docker containers.
- **Secure Communication**: Utilizes SSH for secure communication, ensuring data integrity and confidentiality during transit.
- **Simplified Configuration**: Eliminates the need for complex network configurations or exposing SSH ports on Docker containers.
- **Rust-Powered**: Built with Rust, Tunnyd prioritizes performance, reliability, and safety.

## How It Works
Tunnyd operates as a surrogate SSH daemon, facilitating secure communication between users and Docker containers. Initially, users establish an SSH connection to the real server using valid authentication and credentials configured in the SSH configuration file. Once connected, Tunnyd on the real server acts as an intermediary, redirecting SSH connections to port 2222, where a custom SSH service provided by Tunnyd resides.

Upon connecting to the Tunnyd SSH service, it captures the SSH information and leverages it to execute Docker commands within the targeted container. Tunnyd seamlessly bridges the SSH communication with the Docker container by piping standard input, output, and error streams between the SSH session and the Docker execution process.

This streamlined approach eliminates the need for manual configuration, making remote container access effortless and secure, while maintaining the robust security features provided by SSH.

### Setting up SSH Configurations
Add the following configuration to your `~/.ssh/config` file:

```bash
Host *.my-docker
hostname 192.168.100.100  # Actual server the Docker containers are hosted at
PreferredAuthentications none  # Authentication isn't needed as Tunnyd won't be exposed publicly and will require a ProxyJump to <hostname>
Port 2222
RequestTTY yes
ProxyJump user@hostname
RemoteCommand tunnyd --target %n --user %r
Replace 192.168.100.100 with the actual IP address of the server hosting Docker containers. Make sure to replace user and hostname with the appropriate SSH login credentials and hostname for your environment.
````
Example Docker Compose Configuration
Here's an example of how to configure Docker containers for use with Tunnyd using Docker Compose:

```yaml
version: "3.8"
services:
  app1:
      image:  ubuntu
      container_name: example
      labels:
        - tunnyD.enable=true
        - tunnyD.allowed.users=git,root
        - tunnyD.hostname=my-name.my-docker
  app2:
      image:  ubuntu
      container_name: example-2
      labels:
        - tunnyD.enable=true
        - tunnyD.allowed.users=root
        - tunnyD.hostname=my-media.my-docker
```
Ensure that the labels are correctly set for each Docker container you wish to access remotely using Tunnyd. 
Modify the tunnyD.hostname label to match your custom Docker container naming pattern and adjust the tunnyD.allowed.users label as needed.

## Usage

To use Tunnyd, simply invoke the program with the desired custom name pattern:

```bash
ssh git@my-name.my-docker # You now have a secure shell in app1 container
ssh root@my-media.my-docker  # You now have a secure shell in app2 container
```

Tunnyd will then establish SSH tunnels (via `docker exec`) to Docker containers matching the specified pattern, allowing seamless access to your remote resources.

## Installation

To install Tunnyd, ensure you have Rust installed, then clone the repository and build the project:

```bash
git clone https://github.com/yourusername/tunnyd.git
cd tunnyd
cargo build --release
```

Once built, you can copy the binary to a directory in your PATH for convenient access.

## Contributions

Contributions to Tunnyd are welcome! If you encounter any issues or have ideas for improvements, feel free to open an issue or submit a pull request on the GitHub repository.

## License

Tunnyd is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
