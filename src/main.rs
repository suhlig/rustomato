use clap::{crate_version,AppSettings, Clap};

/// Pomodoro timer
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
}

/// Work with a break
#[derive(Clap)]
struct Break {
}

fn main() {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommands::Pomodoro(_) => {
            println!("Let's do something with a Pomodoro");
        }
        SubCommands::Break(t) => {
            }

            println!("Let's do something with a break");
        }
    }
}
