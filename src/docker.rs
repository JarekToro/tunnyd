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

/// Check validity of labels
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

pub async fn connect_to_docker() -> Result<Docker, Box<dyn std::error::Error>> {
    return match Docker::connect_with_local_defaults() {
        Ok(docker) => {
            info!("Successfully connected to Docker");
            Ok(docker)
        }
        Err(e) => Err(Box::new(e)),
    };
}
