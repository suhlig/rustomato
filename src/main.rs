use clap::{AppSettings, Clap};

/// Pomodoro timer
#[derive(Clap)]
#[clap(version = "0.0.1", author = "Steffen Uhlig <steffen@familie-uhlig.net>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,

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
}

/// Work with a break
#[derive(Clap)]
struct Break {
    /// Print debug info
    #[clap(short, long)]
    debug: bool
}

fn main() {
    let opts: Opts = Opts::parse();

    match opts.verbose {
        0 => { /* nothing printed */ },
        1 => println!("Some verbose info"),
        2 => println!("Tons of verbose info"),
        _ => println!("That's verbose enough"),
    }

    match opts.subcmd {
        SubCommands::Pomodoro(_) => {
            println!("Let's do something with a Pomodoro");
        }
        SubCommands::Break(t) => {
            if t.debug {
                println!("Printing debug info...");
            }

            println!("Let's do something with a break");
        }
    }
}
