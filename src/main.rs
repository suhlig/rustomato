mod rustomato;

use std::{env,process};
use std::path::*;
use clap::{crate_version, AppSettings, Clap};
use rustomato::persistence::Repository;
use rustomato::scheduling::Scheduler;
use rustomato::{Kind, Schedulable, Status};

/// A simple Pomodoro timer for the command line
#[derive(Clap)]
#[clap(version = crate_version!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommands,
}

#[derive(Clap)]
enum SubCommands {
    Pomodoro(PomodoroCommand),
    Break(BreakCommand),
}

/// Work with a pomodoro
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
}

/// Finishes the active Break
#[derive(Clap)]
struct FinishBreak {}

fn main() {
    let location = match env::var("DATABASE_URL") {
        Ok(val) => PathBuf::from(val),
        Err(_) => {
            let mut home = dirs::home_dir().expect("Unable to resolve home directory");
            home.push(".rustomato.db");
            home.to_path_buf()
        }
    };

    println!("Using database {:?}", location); // TODO Only if verbose

    let repo = Repository::new(&location);
    let scheduler = Scheduler::new(repo);
    let pid = process::id();

    match Opts::parse().subcmd {
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(start_pomodoro_options) => {
                let pom =
                    Schedulable::new(pid, Kind::Pomodoro, start_pomodoro_options.duration.into());
                println!("Starting {}", pom); // TODO Only if verbose

                let result = scheduler.run(pom);

                match result {
                    Ok(completed_pom) => {
                        println!("\n{}", completed_pom); // TODO Only if verbose

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
                        eprintln!("Failed to schedule Pomodoro; {}", err);
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

                println!("Starting {}", br3ak); // TODO Only if verbose

                let result = scheduler.run(br3ak);

                match result {
                    Ok(completed_break) => {
                        println!("\n{}", completed_break); // TODO Only if verbose
                        process::exit(0);
                    }
                    Err(err) => {
                        eprintln!("Failed to schedule break; {}", err);
                        process::exit(1);
                    }
                }
            }
        },
    }
}
