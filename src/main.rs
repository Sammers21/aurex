use aurex::{FormatTool, config, manifest};
use clap::{Arg, ArgMatches, Command};
use std::ffi::OsString;
use std::path::Path;

mod ui;

pub fn main() {
    let exit_code = match cli().get_matches().subcommand() {
        Some(("init", _)) => init_command(),
        Some(("add", matches)) => add_command(matches),
        Some(("remove", matches)) => remove_command(matches),
        Some(("build", _)) => build_command(),
        Some(("run", matches)) => run_command(matches),
        Some(("test", _)) | Some(("t", _)) => test_command(),
        Some(("clean", _)) => clean_command(),
        Some(("fmt", _)) => fmt_command(),
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
            "Examples:\n  ax init\n  ax add org.example:demo@1.2.3\n  ax build\n  ax run\n  ax test\n  ax fmt\n  ax java\n  ax help build\n\nProject file:\n  ax reads ax.toml in the current directory.",
        )
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("init")
                .about("Create a new Aurex project in the current directory")
                .long_about(
                    "Create a new Aurex project in the current directory. \
This writes src/com/example/Main.java and ax.toml with package-based main config.",
                )
                .after_help("Examples:\n  ax init"),
        )
        .subcommand(
            Command::new("add")
                .about("Add or update Maven dependencies in ax.toml")
                .long_about(
                    "Add or update one or more Maven dependencies in the current project's \
ax.toml file. Dependency specs use groupId:artifactId@version.",
                )
                .arg(
                    Arg::new("specs")
                        .value_name("GROUP:ARTIFACT@VERSION")
                        .help("Dependency coordinate and version to add")
                        .num_args(1..)
                        .required(true),
                )
                .after_help("Examples:\n  ax add org.apache.commons:commons-lang3@3.14.0\n  ax add com.google.code.gson:gson@2.10.1 info.picocli:picocli@4.7.6"),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove Maven dependencies from ax.toml")
                .long_about(
                    "Remove one or more Maven dependencies from the current project's ax.toml \
file. Dependency specs use groupId:artifactId.",
                )
                .arg(
                    Arg::new("specs")
                        .value_name("GROUP:ARTIFACT")
                        .help("Dependency coordinate to remove")
                        .num_args(1..)
                        .required(true),
                )
                .after_help("Examples:\n  ax remove org.apache.commons:commons-lang3\n  ax remove com.google.code.gson:gson info.picocli:picocli"),
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
            Command::new("test")
                .alias("t")
                .about("Compile and run JUnit 5 tests")
                .long_about(
                    "Compile production sources, compile test sources from the configured \
test root, and run JUnit 5 with junit-platform-console-standalone.",
                )
                .after_help("Examples:\n  ax test\n  ax t\n\nProject file:\n  ax reads ax.toml in the current directory."),
        )
        .subcommand(
            Command::new("clean")
                .about("Remove the target directory")
                .long_about("Remove only the current project's target directory.")
                .after_help("Examples:\n  ax clean"),
        )
        .subcommand(
            Command::new("fmt")
                .about("Format Java sources")
                .long_about(
                    "Format Java sources under the configured production and test roots. \
If eclipse-formatter.xml exists, Aurex uses Eclipse JDT; otherwise it uses google-java-format.",
                )
                .after_help("Examples:\n  ax fmt"),
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

fn add_command(matches: &ArgMatches) -> i32 {
    let specs = values(matches, "specs");
    match manifest::add_dependencies(Path::new("."), &specs) {
        Ok(added) => {
            for spec in added {
                println!("added {}@{}", spec.key, spec.version);
            }
            0
        }
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
}

fn remove_command(matches: &ArgMatches) -> i32 {
    let specs = values(matches, "specs");
    match manifest::remove_dependencies(Path::new("."), &specs) {
        Ok(removed) => {
            for key in removed {
                println!("removed {key}");
            }
            0
        }
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
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

fn test_command() -> i32 {
    let config = match config::try_read_toml(".") {
        Ok(config) => config,
        Err(err) => {
            ui::render_error(&err);
            return 1;
        }
    };
    match aurex::test_project(config) {
        Ok(()) => 0,
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
}

fn clean_command() -> i32 {
    match aurex::clean(".") {
        Ok(true) => {
            println!("removed target");
            0
        }
        Ok(false) => 0,
        Err(err) => {
            ui::render_error(&err);
            1
        }
    }
}

fn fmt_command() -> i32 {
    let config = match config::try_read_toml(".") {
        Ok(config) => config,
        Err(err) => {
            ui::render_error(&err);
            return 1;
        }
    };
    match aurex::format_project(config) {
        Ok(outcome) => {
            match outcome.tool {
                Some(FormatTool::GoogleJavaFormat) => {
                    println!(
                        "formatted {} Java files with google-java-format",
                        outcome.file_count
                    )
                }
                Some(FormatTool::EclipseJdt) => {
                    println!(
                        "formatted {} Java files with Eclipse JDT",
                        outcome.file_count
                    )
                }
                None => println!("no Java files to format"),
            }
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

fn values(matches: &ArgMatches, name: &str) -> Vec<String> {
    matches
        .get_many::<String>(name)
        .map(|values| values.cloned().collect())
        .unwrap_or_default()
}
