use clap::{Arg, Command};
use shlex::Shlex;

fn cli() -> Command {
    Command::new("tunnyD")
        .about("Tunnel into a Docker Container")
        .arg(
            Arg::new("target")
                .short('t')
                .long("target")
                .required(true)
                .value_name("TARGET")
                .help("The hostname that relates to the docker container"),
        )
        .arg(
            Arg::new("user")
                .short('u')
                .long("user")
                .required(false)
                .value_name("USER")
                .help("The user to use to login to the docker container"),
        )
}

/// Represents the arguments for creating a container.
///
/// # Fields
///
/// * `user`: An optional string representing the user for the container.
/// * `target`: A string representing the target for the container.
#[derive(Clone)]
pub struct ContainerArgs {
    pub user: Option<String>,
    pub target: String,
}

/// Parses the given data and matches the arguments.
///
/// # Arguments
///
/// * `data` - A byte slice containing the data to be parsed and matched.
///
/// # Returns
///
/// The matched arguments wrapped in a `ContainerArgs` object.
///
/// # Example
///
/// ```
/// use my_crate::parse_and_match_args;
///
/// let data = b"--user john --target server";
/// let args = parse_and_match_args(data);
/// ```
///
/// # Panics
///
/// This function panics if the required argument "target" is not found.
pub fn parse_and_match_args(data: &[u8]) -> ContainerArgs {
    let data_str = String::from_utf8_lossy(data).into_owned();
    let input = Shlex::new(&data_str);
    let matches = cli().get_matches_from(input);
    // Get the value of user and target
    let (user, target) = (
        matches.get_one::<String>("user").map(|s| s.clone()),
        matches
            .get_one::<String>("target")
            .expect("required")
            .clone(),
    );

    // Return as Args object
    ContainerArgs { user, target }
}
