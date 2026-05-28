use clap::{CommandFactory, Parser, crate_version};
use clap_complete::{Shell, generate};
use rustomato::hooks;
use rustomato::persistence::Repository;
use rustomato::scheduling::{Scheduler, SchedulingError};
use rustomato::{InterruptionKind, Kind, Schedulable, Status};
use std::io;
use std::path::*;
use std::{env, process};
use url::Url;

/// A simple Pomodoro timer for the command line
#[derive(Parser)]
#[clap(version = app_version())]
struct Opts {
    #[clap(short, long)]
    verbose: bool,

    /// Disable hook execution
    #[clap(long)]
    no_hooks: bool,

    #[clap(subcommand)]
    subcmd: SubCommands,
}

#[derive(Parser)]
enum SubCommands {
    /// Initialize the rustomato root directory with sample hooks
    Init(InitCommand),
    Pomodoro(PomodoroCommand),
    Break(BreakCommand),
    Status(StatusCommand),
    Journal(JournalCommand),
    #[clap(hide = true)]
    Completions(CompletionsCommand),
}

/// Initialize rustomato
#[derive(Parser)]
struct InitCommand {}

/// Work with a Pomodoro
#[derive(Parser)]
struct PomodoroCommand {
    #[clap(subcommand)]
    subcmd: PomodoroCommands,
}

#[derive(Parser)]
enum PomodoroCommands {
    Start(StartPomodoro),
    Interrupt(InterruptPomodoro),
    Annotate(AnnotatePomodoro),
}

/// Starts a Pomodoro
#[derive(Parser)]
struct StartPomodoro {
    /// How many minutes this Pomodoro should last
    #[clap(
        short,
        long,
        required(false),
        default_value("25"),
        value_name("DURATION")
    )]
    duration: u8,
}

/// Finishes the active Pomodoro
#[derive(Parser)]
struct FinishPomodoro {}

/// Marks the active Pomodoro as interrupted
#[derive(Parser)]
struct InterruptPomodoro {
    /// Whether the interruption is internal (self-inflicted) or external (environmental)
    #[clap(short, long, default_value = "internal", value_name = "KIND")]
    kind: String,
}

/// Annotates a Pomodoro
#[derive(Parser)]
struct AnnotatePomodoro {
    /// The annotation text. Reads from STDIN if not provided.
    words: Vec<String>,
}

/// Work with a break
#[derive(Parser)]
struct BreakCommand {
    #[clap(subcommand)]
    subcmd: BreakCommands,
}

#[derive(Parser)]
enum BreakCommands {
    Start(StartBreak),
    Annotate(AnnotateBreak),
}

/// Starts a break
#[derive(Parser)]
struct StartBreak {
    /// How many minutes this break should last
    #[clap(
        short,
        long,
        required(false),
        default_value("5"),
        value_name("DURATION")
    )]
    duration: u8,

    /// Cancel whatever may currently be running before starting the break
    #[clap(short, long)]
    force: bool,
}

/// Annotates a Break
#[derive(Parser)]
struct AnnotateBreak {
    /// The annotation text. Reads from STDIN if not provided.
    words: Vec<String>,
}

/// Finishes the active Break
#[derive(Parser)]
struct FinishBreak {}

/// Report status
#[derive(Parser)]
struct StatusCommand {}

/// Show today's journal of pomodori and breaks
#[derive(Parser)]
struct JournalCommand {}

/// Generate shell completions
#[derive(Parser)]
struct CompletionsCommand {
    /// The shell to generate completions for
    #[clap(value_enum)]
    shell: Shell,
}

fn main() {
    let opts = Opts::parse();

    // The completions subcommand doesn't need a database, so handle it early
    if let SubCommands::Completions(completions_options) = &opts.subcmd {
        let mut cmd = Opts::command();
        generate(
            completions_options.shell,
            &mut cmd,
            "rustomato",
            &mut io::stdout(),
        );
        return;
    }

    // TODO Use Clap's `env` option
    let root = match env::var("RUSTOMATO_ROOT") {
        Ok(val) => PathBuf::from(val),
        Err(_) => {
            let mut root = dirs::home_dir().expect("resolving the home directory");
            root.push(".rustomato/");

            if !root.exists() {
                std::fs::create_dir(root.as_path()).expect("creating the root directory");
            }

            root
        }
    };

    let verbose = opts.verbose;

    if verbose {
        println!("Using root {}", root.to_str().expect("converting"));
    }

    // Handle init early — no database needed.
    if let SubCommands::Init(_) = &opts.subcmd {
        match hooks::init(&root) {
            Ok(()) => {
                println!(
                    "Initialized rustomato in {}",
                    root.to_str().expect("converting")
                );
                println!(
                    "Sample hooks created in {}/hooks",
                    root.to_str().expect("converting")
                );
            }
            Err(e) => {
                eprintln!("Error: failed to initialize rustomato: {}", e);
                process::exit(1);
            }
        }
        return;
    }

    let db_url = match env::var("RUSTOMATO_DATABASE_URL") {
        Ok(val) => Url::parse(&val).expect("parsing the database URL"),
        Err(_) => {
            let db_path = root.join("data.db");
            Url::from_file_path(&db_path).expect("converting database path to URL")
        }
    };

    if verbose {
        println!("Using database URL {}", db_url);
    }

    let repo = Repository::from_url(&db_url);
    let scheduler = Scheduler::new(repo, root, verbose, opts.no_hooks);
    let pid = process::id();

    match opts.subcmd {
        SubCommands::Init(_) => unreachable!(), // handled above
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
                            SchedulingError::AlreadyRunning(pid) => eprintln!(
                                "Error: {}. Wait for the currently active pid {} to end, cancel it, or use --force.",
                                err, pid
                            ),
                            SchedulingError::HookRejected => {
                                // Error message already printed by the scheduler
                                process::exit(1);
                            }
                            _ => eprintln!("Error: {}.", err),
                        }
                        process::exit(1);
                    }
                }
            }
            PomodoroCommands::Interrupt(interrupt_options) => {
                let kind = match InterruptionKind::from(&interrupt_options.kind) {
                    Ok(k) => k,
                    Err(e) => {
                        eprintln!("Error: {}.", e);
                        process::exit(1);
                    }
                };

                match scheduler.interrupt(kind) {
                    Ok(interrupted) => {
                        if verbose {
                            println!("{}", interrupted);
                        }
                        process::exit(0);
                    }
                    Err(err) => {
                        eprintln!("Error: {}.", err);
                        process::exit(1);
                    }
                }
            }
            PomodoroCommands::Annotate(annotate_options) => {
                let text = annotation_text(&annotate_options.words);
                if text.is_empty() {
                    eprintln!("Error: annotation text is empty.");
                    process::exit(1);
                }

                if verbose {
                    println!("Annotating with '{}'", text);
                }

                match scheduler.annotate(&text) {
                    Ok(annotation) => {
                        if verbose {
                            println!("Annotated {}", annotation.body);
                        }
                    }
                    Err(err) => {
                        eprintln!("Error: {}.", err);
                        process::exit(1);
                    }
                }
            }
        },
        SubCommands::Status(_) => {
            // TODO Re-use repo, but this will require a better understanding of lifetimes
            match Repository::from_url(&db_url).active() {
                Ok(schedulable) => match schedulable {
                    Some(existing) => println!("{}", existing),
                    None => println!("Nothing active"),
                },
                Err(e) => {
                    eprintln!("{}", e)
                }
            }
        }
        SubCommands::Journal(_) => {
            use chrono::Local;

            match Repository::from_url(&db_url).today() {
                Ok(entries) => {
                    if entries.is_empty() {
                        println!("No entries for today.");
                    } else {
                        let today = Local::now().date_naive();
                        println!("Journal for {}:\n", today);

                        for entry in &entries {
                            let start = rustomato::format_timestamp(entry.started_at);
                            let end = if entry.finished_at != 0 {
                                rustomato::format_timestamp(entry.finished_at)
                            } else if entry.cancelled_at != 0 {
                                rustomato::format_timestamp(entry.cancelled_at)
                            } else {
                                "...".to_string()
                            };

                            let status_icon = match entry.status() {
                                Status::Finished => "[finished]",
                                Status::Cancelled => "[cancelled]",
                                Status::Active => "[active]",
                                Status::Stale => "[stale]",
                                Status::New => "?",
                            };

                            let interrupt_info = if entry.interruptions > 0 {
                                format!(
                                    " ({} interruption{})",
                                    entry.interruptions,
                                    if entry.interruptions == 1 { "" } else { "s" }
                                )
                            } else {
                                String::new()
                            };
                            println!(
                                "  {:>8} - {:<8}  {:<10} ({:>2} min)  {}{}",
                                start,
                                end,
                                format!("{}", entry.kind),
                                entry.duration,
                                status_icon,
                                interrupt_info
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    process::exit(1);
                }
            }
        }
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
                            SchedulingError::AlreadyRunning(pid) => eprintln!(
                                "Error: {}. Wait for the currently active pid {} to end, cancel it, or use --force.",
                                err, pid
                            ),
                            SchedulingError::HookRejected => {
                                process::exit(1);
                            }
                            _ => eprintln!("Error: {}.", err),
                        }
                        process::exit(1);
                    }
                }
            }
            BreakCommands::Annotate(annotate_options) => {
                let text = annotation_text(&annotate_options.words);
                if text.is_empty() {
                    eprintln!("Error: annotation text is empty.");
                    process::exit(1);
                }

                if verbose {
                    println!("Annotating with '{}'", text);
                }

                match scheduler.annotate(&text) {
                    Ok(annotation) => {
                        if verbose {
                            println!("Annotated {}", annotation.body);
                        }
                    }
                    Err(err) => {
                        eprintln!("Error: {}.", err);
                        process::exit(1);
                    }
                }
            }
        },
        SubCommands::Completions(_) => unreachable!(),
    };
}

/// Read annotation text from positional args or stdin.
fn annotation_text(words: &[String]) -> String {
    if !words.is_empty() {
        words.join(" ")
    } else {
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin()
            .lock()
            .read_to_string(&mut input)
            .unwrap_or_default();
        input.trim().to_string()
    }
}

/// Provides the app version at build time - either the current git version, or, if not available, the static version string of the crate.
fn app_version() -> &'static str {
    match built_info::GIT_VERSION {
        Some(g) => g,
        None => crate_version!(),
    }
}

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs")); // The file has been placed there by the build script.
}
