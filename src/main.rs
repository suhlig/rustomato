use clap::{crate_version, AppSettings, Clap};
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::{process, thread, time::Duration, time::Instant};

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
    Pomodoro(Pomodoro),
    Break(Break),
}

/// Work with a pomodoro
#[derive(Clap)]
struct Pomodoro {
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
struct Break {
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

                match waiter(start_options.duration.into()).recv() {
                    Ok(cancelled) => {
                        if cancelled {
                            println!("Pomodoro was cancelled");
                            process::exit(1);
                        } else {
                            println!("Finished the Pomodoro");
                            process::exit(0);
                        }
                    }
                    Err(_) => {
                        println!("Error: not sure what happened")
                    }
                }
            }
            PomodoroCommands::Interrupt(_) => {
                println!("Marking the active Pomodoro as interrupted");
            }
            PomodoroCommands::Cancel(_) => {
                println!("Cancelling the active Pomodoro");
            }
            PomodoroCommands::Finish(_) => {
                println!("Finishing the active Pomodoro");
            }
        },
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(start_options) => {
                println!(
                    "Starting a new break that will last for {} minutes",
                    start_options.duration
                );

                match waiter(start_options.duration.into()).recv() {
                    Ok(cancelled) => {
                        if cancelled {
                            println!("Break was cancelled");
                            process::exit(1);
                        } else {
                            println!("Finished the break");
                            process::exit(0);
                        }
                    }
                    Err(_) => {
                        println!("Error: not sure what happened")
                    }
                }
            }
            BreakCommands::Finish(_) => {
                println!("Finishing the active break");
            }
        },
    }
}

fn waiter(duration: u64) -> Receiver<bool> {
    let (control_tx, control_rx) = channel();
    let (result_tx, result_rx) = channel::<bool>();

    ctrlc::set_handler(move || {
        control_tx
            .send(())
            .expect("Could not send signal on control channel.")
    })
    .expect("Error setting Ctrl-C handler");

    thread::spawn({
        move || {
            let mut done = false;
            let break_duration = Duration::new(60 * duration, 0);
            let start = Instant::now();

            while !done {
                if start.elapsed() > break_duration {
                    done = true;
                    result_tx.send(false).expect("could not send result");
                }

                match control_rx.try_recv() {
                    Ok(_) => {
                        done = true;
                        result_tx.send(true).expect("could not send result")
                    }
                    Err(TryRecvError::Disconnected) => {
                        println!("Error: channel disconnected");
                        done = true;
                    }
                    Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(25)),
                }
            }
        }
    })
    .join()
    .unwrap();
    return result_rx;
}
