use clap::{Arg, Command};
use shlex::Shlex;

pub fn cli() -> Command {
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

// Define a new struct to hold the user and target values
#[derive(Clone)]
pub struct ContainerArgs {
    pub user: Option<String>,
    pub target: String,
}

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
