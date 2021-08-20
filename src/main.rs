mod rustomato;

use clap::{crate_version, AppSettings, Clap};
use rustomato::persistence::Repository;
use rustomato::scheduling::Scheduler;
use rustomato::{Schedulable, Status, Kind};
use std::path::Path;
use std::process;

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
    // TODO This is ugly
    let home = dirs::home_dir().expect("Unable to find home directory");
    let home_home = home.to_str().expect("Unable to convert to string");
    let location = Path::new(home_home).join(".rustomato.sqlite3");

    let repo = Repository::new(&location);
    let scheduler = Scheduler::new(repo);

    match Opts::parse().subcmd {
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(start_pomodoro_options) => {
                let pom = Schedulable::new(Kind::Pomodoro, start_pomodoro_options.duration.into());
                println!("Starting new {}", pom); // TODO Only if verbose

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
                        println!("Failed to schedule {}", err); // TODO Only if verbose
                        process::exit(1);
                    }
                }
            }
            PomodoroCommands::Interrupt(_) => {
                println!("TODO Marking the active Pomodoro as interrupted");
            }
            PomodoroCommands::Annotate(_) => {
                println!("TODO Annotating the active Pomodoro");
            }
        },
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(start_break_options) => {
                let br3ak = Schedulable::new(Kind::Break, start_break_options.duration.into());

                println!("Starting {}", br3ak); // TODO Only if verbose

                let result = scheduler.run(br3ak);

                match result {
                    Ok(completed_break) => {
                        println!("\n{}", completed_break); // TODO Only if verbose
                        process::exit(0);
                    }
                    Err(err) => {
                        println!("Failed to schedule: {}", err); // TODO Only if verbose
                        process::exit(1);
                    }
                }
            }
        },
    }
}
