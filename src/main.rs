use aurex::config;

pub fn main() {
    match clap::Command::new("ax")
        .bin_name("ax")
        .about("Aurex Java build system")
        .subcommand_required(true)
        .subcommand(clap::command!("init"))
        .subcommand(clap::command!("build"))
        .subcommand(clap::command!("run"))
        .subcommand(clap::command!("java").about("Print the Java runtime Aurex uses"))
        .get_matches()
        .subcommand()
    {
        Some(("init", _)) => {
            aurex::init(".");
        }
        Some(("build", _)) => {
            aurex::build(config::read_toml("."));
        }
        Some(("run", _)) => {
            aurex::run(config::read_toml("."));
        }
        Some(("java", _)) => {
            if let Err(err) = aurex::java() {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        _ => println!("No subcommand provided"),
    };
}
