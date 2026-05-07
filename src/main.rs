use aurex::config;
use clap::{Arg, ArgMatches, Command};
use std::ffi::OsString;

mod ui;

pub fn main() {
    let exit_code = match cli().get_matches().subcommand() {
        Some(("init", _)) => init_command(),
        Some(("build", _)) => build_command(),
        Some(("run", matches)) => run_command(matches),
        Some(("java", _)) => java_command(),
        _ => 1,
    };
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn cli() -> Command {
    Command::new("ax")
        .bin_name("ax")
        .about("Aurex Java build system")
        .long_about(
            "Aurex (ax) builds small Java applications from an ax.toml file. \
It resolves Maven dependencies, compiles sources with javac, copies resources, \
and writes a runnable jar.",
        )
        .after_help(
            "Examples:\n  ax init\n  ax build\n  ax run\n  ax java\n  ax help build\n\nProject file:\n  ax reads ax.toml in the current directory.",
        )
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("init")
                .about("Create a new Aurex project in the current directory")
                .long_about(
                    "Create a new Aurex project in the current directory. \
This writes src/com/example/Main.java and ax.toml.",
                )
                .after_help("Examples:\n  ax init"),
        )
        .subcommand(
            Command::new("build")
                .about("Compile sources and package the project jar")
                .long_about(
                    "Compile Java sources, resolve Maven dependencies, copy configured resources, \
and package the current project into a runnable jar.",
                )
                .after_help("Examples:\n  ax build\n\nProject file:\n  ax reads ax.toml in the current directory."),
        )
        .subcommand(
            Command::new("run")
                .about("Build the project and run the jar")
                .long_about(
                    "Build the current project, then run the produced jar with java -jar.",
                )
                .arg(
                    Arg::new("args")
                        .value_name("ARG")
                        .help("Arguments passed to the Java application's main method")
                        .num_args(0..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true)
                        .value_parser(clap::value_parser!(OsString)),
                )
                .after_help("Examples:\n  ax run\n  ax run --config config.prod.yaml\n\nProject file:\n  ax reads ax.toml in the current directory."),
        )
        .subcommand(
            Command::new("java")
                .about("Print the Java runtime Aurex uses")
                .long_about(
                    "Print the Java runtime that Aurex resolves from the current shell PATH.",
                )
                .after_help("Examples:\n  ax java"),
        )
}

fn init_command() -> i32 {
    match aurex::try_init(".") {
        Ok(()) => {
            ui::render_init_success(".");
            0
        }
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
}

fn build_command() -> i32 {
    let config = match config::try_read_toml(".") {
        Ok(config) => config,
        Err(err) => {
            ui::render_error(&err);
            return 1;
        }
    };
    let artifact = config.jar_file();
    let mut reporter = ui::CliBuildReporter::new("ax build", artifact, ui::BuildUiStyle::Full);
    match aurex::build_with_reporter(config, &mut reporter) {
        Ok(_) => {
            reporter.finish_success();
            0
        }
        Err(err) => {
            reporter.finish_error(&err);
            1
        }
    }
}

fn run_command(matches: &ArgMatches) -> i32 {
    let config = match config::try_read_toml(".") {
        Ok(config) => config,
        Err(err) => {
            ui::render_error(&err);
            return 1;
        }
    };
    let app_args: Vec<OsString> = matches
        .get_many::<OsString>("args")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let artifact = config.jar_file();
    let mut reporter = ui::CliBuildReporter::new("ax run", artifact, ui::BuildUiStyle::Quiet);
    match aurex::run_with_reporter_args(config, app_args, &mut reporter) {
        Ok(()) => {
            reporter.finish_success();
            0
        }
        Err(err) => {
            reporter.finish_error(&err);
            1
        }
    }
}

fn java_command() -> i32 {
    match aurex::java_info() {
        Ok(info) => {
            ui::render_java_info(&info);
            0
        }
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
}
