use bollard::container::ListContainersOptions;
use bollard::errors::Error;
use bollard::models::ContainerSummary;
use bollard::Docker;
use log::info;
use std::collections::HashMap;

use crate::cli::ContainerArgs;

const LIST_ALL_CONTAINERS: bool = true;
const SSH_ENABLE_LABEL_KEY: &str = "tunnyD.enable";
const SSH_HOSTNAME_LABEL_KEY: &str = "tunnyD.hostname";
const SSH_ALLOWED_USERS_LABEL_KEY: &str = "tunnyD.allowed.users";
const EXEC_DOCKER: &str = "docker";
const SSH_COMMAND_ENV: &str = "SSH_ORIGINAL_COMMAND=${}";
const COMMAND_SHELL: &str = "sh";

/// Checks the validity of a container based on its labels, target, and user.
///
/// # Arguments
///
/// * `labels` - A HashMap of labels associated with the container.
/// * `target` - The target label value to match against the SSH hostname label.
/// * `user` - The user label value to match against the allowed users label.
///
/// # Returns
///
/// Returns a boolean indicating whether the container is valid or not.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// let labels = {
///     let mut hashmap = HashMap::new();
///     hashmap.insert(String::from("ssh-enable"), String::from("true"));
///     hashmap.insert(String::from("ssh-hostname"), String::from("myhost"));
///     hashmap.insert(String::from("ssh-allowed-users"), String::from("user1,user2"));
///     hashmap
/// };
///
/// assert_eq!(true, check_container_validity(&labels, "myhost", "user1"));
/// assert_eq!(false, check_container_validity(&labels, "otherhost", "user3"));
/// ```
fn check_container_validity(labels: &HashMap<String, String>, target: &str, user: &str) -> bool {
    if let Some(value) = labels.get(SSH_ENABLE_LABEL_KEY) {
        // Assuming value for SSH_ALLOWED_USERS_LABEL_KEY is comma separated
        let allow_users = labels
            .get(SSH_ALLOWED_USERS_LABEL_KEY)
            .map_or(Vec::new(), |users| {
                users
                    .split(',')
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            });
        value == "true"
            && labels
                .get(SSH_HOSTNAME_LABEL_KEY)
                .unwrap_or(&String::from(""))
                == target
            && (allow_users.is_empty()
                || (!user.is_empty() && allow_users.contains(&user.to_string())))
    } else {
        false
    }
}

/// Finds an SSH-enabled container based on the provided arguments.
///
/// # Arguments
///
/// * `args` - The arguments used to filter the containers.
///
/// # Returns
///
/// * `Result<ContainerSummary, Error>` - The container summary if a match is found, otherwise an error.
///
/// # Examples
///
/// ```rust
/// use my_crate::ContainerArgs;
/// use futures::executor::block_on;
///
/// let args = ContainerArgs {
///     target: "name_matching_docker_label_tunnyD.hostname",
///     user: "root",
/// };
///
/// let result = find_ssh_enabled_container(&args).await;
/// ```
pub async fn find_ssh_enabled_container(args: &ContainerArgs) -> Result<ContainerSummary, Error> {
    let docker = connect_to_docker().await.expect("get docker");
    let options = ListContainersOptions::<String> {
        all: LIST_ALL_CONTAINERS,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(options)).await?;
    for container in containers {
        match &container.labels {
            None => continue,
            Some(labels) => {
                if check_container_validity(
                    &labels,
                    &args.target,
                    &args.user.clone().unwrap_or_default(),
                ) {
                    return Ok(container);
                }
            }
        }

        // let container_id = &container.id.expect("Missing Container Id");

        // exec_into_container(&args, &container_id);
    }
    Err(Error::DockerContainerWaitError {
        error: "No Available Container matches".to_string(),
        code: 0,
    })
}
//
// fn exec_into_container(args: &ContainerArgs, container_id: &&String) {
//     let ssh_original_command = format!(
//         "SSH_ORIGINAL_COMMAND={}",
//         std::env::var("SSH_ORIGINAL_COMMAND").unwrap_or("\"\"".parse().unwrap())
//     );
//
//     let mut docker_args = vec![
//         "exec",
//         "-i",
//         "--env",
//         &ssh_original_command,
//         &container_id.as_str(),
//         COMMAND_SHELL,
//     ];
//
//     if let Some(user) = &args.user {
//         // only add args.user if it is not None
//         if !user.is_empty() {
//             // only add user if it is not an empty string
//             docker_args.insert(2, "-u");
//             docker_args.insert(3, user.as_str());
//         }
//     }
//     let args_to_pass: Vec<String> = env::args().skip(1).collect();
//     docker_args.extend(args_to_pass.iter().map(|s| s.as_str()));
//     let docker_args_str = docker_args.join(" ");
//
//     // Print or log the full command
//     println!("Full command: {} {}", EXEC_DOCKER, docker_args_str);
//     Command::new(EXEC_DOCKER)
//         .args(&docker_args)
//         .spawn()
//         .expect("Failed to execute command");
// }

/// Connects to Docker using the local defaults.
///
/// # Returns
///
/// Returns a `Result` containing a `Docker` instance if the connection is successful.
/// If there is an error during the connection, the error is wrapped in a `Box<dyn std::error::Error>`.
///
/// # Examples
///
/// ```rust
/// use docker::Docker;
///
/// #[tokio::main]
/// async fn main() {
///     match connect_to_docker().await {
///         Ok(docker) => {
///             println!("Connected to Docker successfully!");
///             // Use the Docker instance here
///         }
///         Err(e) => {
///             eprintln!("Failed to connect to Docker: {}", e);
///         }
///     }
/// }
/// ```
pub async fn connect_to_docker() -> Result<Docker, Box<dyn std::error::Error>> {
    return match Docker::connect_with_local_defaults() {
        Ok(docker) => {
            info!("Successfully connected to Docker");
            Ok(docker)
        }
        Err(e) => Err(Box::new(e)),
    };
}
