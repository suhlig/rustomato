use clap::{crate_version, AppSettings, Clap};
use rustomato::persistence::Repository;
use rustomato::scheduling::{Scheduler, SchedulingError};
use rustomato::{Kind, Schedulable, Status};
use std::path::*;
use std::{env, process};
use url::Url;

/// A simple Pomodoro timer for the command line
#[derive(Clap)]
#[clap(version = crate_version!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long, takes_value(false))]
    verbose: bool,
    #[clap(subcommand)]
    subcmd: SubCommands,
}

#[derive(Clap)]
enum SubCommands {
    Pomodoro(PomodoroCommand),
    Break(BreakCommand),
}

/// Work with a Pomodoro
#[derive(Clap)]
struct PomodoroCommand {
    #[clap(subcommand)]
    subcmd: PomodoroCommands,
}

#[derive(Clap)]
enum PomodoroCommands {
    Start(StartPomodoro),
    Interrupt(InterruptPomodoro),
    Annotate(AnnotatePomodoro),
}

/// Starts a Pomodoro
#[derive(Clap)]
struct StartPomodoro {
    /// How many minutes this Pomodoro should last
    #[clap(
        short,
        long,
        required(false),
        default_value("25"),
        takes_value(true),
        value_name("DURATION")
    )]
    duration: u8,
}

/// Finishes the active Pomodoro
#[derive(Clap)]
struct FinishPomodoro {}

/// Marks the active Pomodoro as interrupted
#[derive(Clap)]
struct InterruptPomodoro {}

/// Annotates a Pomodoro
#[derive(Clap)]
struct AnnotatePomodoro {}

/// Work with a break
#[derive(Clap)]
struct BreakCommand {
    #[clap(subcommand)]
    subcmd: BreakCommands,
}

#[derive(Clap)]
enum BreakCommands {
    Start(StartBreak),
}

/// Starts a break
#[derive(Clap)]
struct StartBreak {
    /// How many minutes this break should last
    #[clap(
        short,
        long,
        required(false),
        default_value("5"),
        takes_value(true),
        value_name("DURATION")
    )]
    duration: u8,

    /// Cancel whatever may currently be running before starting the break
    #[clap(short, long)]
    force: bool,
}

/// Finishes the active Break
#[derive(Clap)]
struct FinishBreak {}

fn main() {
    // TODO Use Clap's `env` option
    let root = match env::var("RUSTOMATO_ROOT") {
        Ok(val) => PathBuf::from(val),
        Err(_) => {
            let mut root = dirs::home_dir().expect("resolving the home directory");
            root.push(".rustomato/");

            if !root.exists() {
                std::fs::create_dir(root.as_path()).expect("creating the root directory");
            }

            root.to_path_buf()
        }
    };

    let verbose = Opts::parse().verbose;

    if verbose {
        println!("Using root {}", root.to_str().expect("converting"));
    }

    let db_url = match env::var("RUSTOMATO_DATABASE_URL") {
        Ok(val) => Url::parse(&String::from(val)).expect("parsing the database URL"),
        Err(_) => {
            let base = Url::parse("file://").expect("parsing the base URL");
            let with_dir = base
                .join(root.to_str().expect("converting root to string"))
                .expect("appending the root directory");
            with_dir.join("data.db").expect("parsing the database URL")
        }
    };

    if verbose {
        println!("Using database URL {}", db_url);
    }

    let repo = Repository::from_url(&db_url);
    let scheduler = Scheduler::new(repo);
    let pid = process::id();

    match Opts::parse().subcmd {
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(start_pomodoro_options) => {
                let pom =
                    Schedulable::new(pid, Kind::Pomodoro, start_pomodoro_options.duration.into());

                if verbose {
                    println!("Starting {}", pom);
                }

                match scheduler.run(pom) {
                    Ok(completed_pom) => {
                        if verbose {
                            println!("\n{}", completed_pom);
                        }

                        match completed_pom.status() {
                            Status::Cancelled => {
                                process::exit(1);
                            }
                            Status::Finished => {
                                process::exit(0);
                            }
                            _ => (), // TODO Should not happen; panic?
                        }
                    }
                    Err(err) => {
                        match err {
                            SchedulingError::AlreadyRunning(pid) => eprintln!("Error: {}. Wait for the currently active pid {} to end, cancel it, or use --force.", err, pid),
                            _ => eprintln!("Error: {}.", err),
                        }
                        process::exit(1);
                    }
                }
            }
            PomodoroCommands::Interrupt(_) => {
                eprintln!("TODO Marking the active Pomodoro as interrupted");
            }
            PomodoroCommands::Annotate(_) => {
                eprintln!("TODO Annotating the active Pomodoro");
            }
        },
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(start_break_options) => {
                let br3ak = Schedulable::new(pid, Kind::Break, start_break_options.duration.into());

                if verbose {
                    println!("Starting {}", br3ak);
                }

                match scheduler.run(br3ak) {
                    Ok(completed_break) => {
                        if verbose {
                            println!("\n{}", completed_break);
                        }
                        process::exit(0);
                    }
                    Err(err) => {
                        match err {
                            SchedulingError::AlreadyRunning(pid) => eprintln!("Error: {}. Wait for the currently active pid {} to end, cancel it, or use --force.", err, pid),
                            _ => eprintln!("Error: {}.", err),
                        }
                        process::exit(1);
                    }
                }
            }
        },
    }
}
