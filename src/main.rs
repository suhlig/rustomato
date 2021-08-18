mod rustomato;

use clap::{crate_version, AppSettings, Clap};
use std::process;
use rustomato::{Pomodoro, Break};

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
    Cancel(CancelPomodoro),
    Finish(FinishPomodoro),
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

/// Cancels the active Pomodoro
#[derive(Clap)]
struct CancelPomodoro {}

/// Work with a break
#[derive(Clap)]
struct BreakCommand {
    #[clap(subcommand)]
    subcmd: BreakCommands,
}

#[derive(Clap)]
enum BreakCommands {
    Start(StartBreak),
    Finish(FinishBreak),
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
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(start_options) => {
                println!(
                    "Starting a new Pomodoro that will last for {} minutes",
                    start_options.duration
                );

                let pom = Pomodoro::new(start_options.duration.into());

                if pom.run() {
                    println!("Finished the Pomodoro {}", pom.uuid);
                    process::exit(0);
                } else {
                    println!("\nPomodoro {} was cancelled", pom.uuid);
                    process::exit(1);
                }
            }
            PomodoroCommands::Interrupt(_) => {
                println!("TODO Marking the active Pomodoro as interrupted");
            }
            PomodoroCommands::Cancel(_) => {
                println!("TODO Cancelling the active Pomodoro");
            }
            PomodoroCommands::Finish(_) => {
                println!("TODO Finishing the active Pomodoro");
            }
        },
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(start_options) => {
                println!(
                    "Starting a new break that will last for {} minutes",
                    start_options.duration
                );

                let br3ak = Break::new(start_options.duration.into());

                if br3ak.run() {
                    println!("Finished the break {}", br3ak.uuid);
                    process::exit(0);
                } else {
                    println!("\nBreak {} was cancelled", br3ak.uuid);
                    process::exit(1);
                }
            }
            BreakCommands::Finish(_) => {
                println!("Finishing the active break");
            }
        },
    }
}
