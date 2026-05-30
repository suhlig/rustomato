use clap::{CommandFactory, Parser, crate_version};
use clap_complete::{Shell, generate};
use rustomato::hooks;
use rustomato::persistence::Repository;
use rustomato::scheduling::{Scheduler, SchedulingError};
use rustomato::{Annotation, InterruptionKind, Kind, Schedulable, Status};
use std::io;
use std::path::*;
use std::{env, process};
use url::Url;

/// A simple Pomodoro timer for the command line
#[derive(Parser)]
#[clap(version = app_version(), infer_subcommands = true)]
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
    /// Generate a productivity report
    Report(ReportCommand),
    #[clap(hide = true)]
    Completions(CompletionsCommand),
}

/// Initialize rustomato
#[derive(Parser)]
struct InitCommand {}

/// Work with a Pomodoro
#[derive(Parser)]
#[clap(infer_subcommands = true)]
struct PomodoroCommand {
    #[clap(subcommand)]
    subcmd: PomodoroCommands,
}

#[derive(Parser)]
enum PomodoroCommands {
    Start(StartPomodoro),
    Interrupt(InterruptPomodoro),
    Annotate(AnnotatePomodoro),
    Log(LogPomodoro),
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

    /// Cancel whatever may currently be running before starting the pomodoro
    #[clap(short, long)]
    force: bool,
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

/// Log an externally completed pomodoro
#[derive(Parser)]
struct LogPomodoro {
    /// When the pomodoro started (RFC 3339 / ISO 8601)
    #[clap(long, value_name = "TIMESTAMP")]
    started_at: Option<String>,

    /// When the pomodoro finished (RFC 3339 / ISO 8601)
    #[clap(long, value_name = "TIMESTAMP")]
    finished_at: Option<String>,

    /// Duration in minutes (default: 25). Cannot be used when both --started-at and --finished-at are given.
    #[clap(short, long, value_name = "MINUTES")]
    duration: Option<u8>,
}

/// Annotates a Pomodoro
#[derive(Parser)]
struct AnnotatePomodoro {
    /// The annotation text. Reads from STDIN if not provided.
    words: Vec<String>,
}

/// Work with a break
#[derive(Parser)]
#[clap(infer_subcommands = true)]
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

/// Generate shell completions
#[derive(Parser)]
struct CompletionsCommand {
    /// The shell to generate completions for
    #[clap(value_enum)]
    shell: Shell,
}

/// Generate a productivity report
#[derive(Parser)]
#[clap(infer_subcommands = true)]
struct ReportCommand {
    #[clap(subcommand)]
    subcmd: ReportCommands,
}

#[derive(Parser)]
enum ReportCommands {
    Day(DayReport),
    Week(WeekReport),
    /// Monthly productivity report with week-by-week breakdown
    Month(MonthReport),
    /// Rolling window productivity report
    Last(LastReport),
    /// Interruption pattern analysis by hour of day and day of week
    Interruptions(InterruptionsReport),
}

/// Daily productivity report
#[derive(Parser)]
struct DayReport {
    /// Date in ISO 8601 format (YYYY-MM-DD). Defaults to today.
    #[clap(long, value_name = "DATE")]
    date: Option<String>,
}

/// Weekly productivity report
#[derive(Parser)]
struct WeekReport {
    /// A date within the target week (YYYY-MM-DD). Defaults to today.
    #[clap(long, value_name = "DATE")]
    date: Option<String>,
}

/// Monthly productivity report
#[derive(Parser)]
struct MonthReport {
    /// A date within the target month (YYYY-MM or YYYY-MM-DD). Defaults to the current month.
    #[clap(long, value_name = "DATE")]
    date: Option<String>,
    /// Number of months to show including this one (for trend comparison). Defaults to 3.
    #[clap(long, default_value = "3", value_name = "MONTHS")]
    months: u32,
}

/// Rolling window report (last N days)
#[derive(Parser)]
struct LastReport {
    /// End date for the window (YYYY-MM-DD). Defaults to today.
    #[clap(long, value_name = "DATE")]
    date: Option<String>,
    /// Size of the window in days. Defaults to 7.
    #[clap(long, default_value = "7", value_name = "DAYS")]
    days: u32,
}

/// Interruption pattern report
#[derive(Parser)]
struct InterruptionsReport {
    /// End date for the analysis window (YYYY-MM-DD). Defaults to today.
    #[clap(long, value_name = "DATE")]
    date: Option<String>,
    /// Number of days to look back. Defaults to 7.
    #[clap(long, default_value = "7", value_name = "DAYS")]
    days: u32,
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

                match scheduler.run(pom, start_pomodoro_options.force) {
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
            PomodoroCommands::Log(log_options) => {
                let (started_at, finished_at) = match (
                    &log_options.started_at,
                    &log_options.finished_at,
                    log_options.duration,
                ) {
                    (Some(s), None, dur) => {
                        let dur = dur.unwrap_or(25) as i64;
                        let started_at = rustomato::parse_timestamp(s).unwrap_or_else(|e| {
                            eprintln!("Error: {} --started-at: {}", e, s);
                            process::exit(1);
                        });
                        let finished_at = started_at + dur * 60;
                        (started_at, finished_at)
                    }
                    (None, Some(f), dur) => {
                        let dur = dur.unwrap_or(25) as i64;
                        let finished_at = rustomato::parse_timestamp(f).unwrap_or_else(|e| {
                            eprintln!("Error: {} --finished-at: {}", e, f);
                            process::exit(1);
                        });
                        let started_at = finished_at - dur * 60;
                        (started_at, finished_at)
                    }
                    (Some(s), Some(f), None) => {
                        let started_at = rustomato::parse_timestamp(s).unwrap_or_else(|e| {
                            eprintln!("Error: {} --started-at: {}", e, s);
                            process::exit(1);
                        });
                        let finished_at = rustomato::parse_timestamp(f).unwrap_or_else(|e| {
                            eprintln!("Error: {} --finished-at: {}", e, f);
                            process::exit(1);
                        });
                        (started_at, finished_at)
                    }
                    (Some(_), Some(_), Some(_)) => {
                        eprintln!(
                            "Error: cannot specify --duration when both --started-at and --finished-at are given."
                        );
                        process::exit(1);
                    }
                    (None, None, _) => {
                        eprintln!(
                            "Error: at least one of --started-at or --finished-at is required."
                        );
                        process::exit(1);
                    }
                };

                if finished_at < started_at {
                    eprintln!("Error: --finished-at must be after --started-at.");
                    process::exit(1);
                }

                let actual_duration = (finished_at - started_at) / 60;

                if verbose {
                    println!(
                        "Logging externally completed pomodoro ({} min)",
                        actual_duration
                    );
                }

                let mut pom = Schedulable::new(0, Kind::Pomodoro, actual_duration);
                pom.started_at = started_at;
                pom.finished_at = finished_at;

                match scheduler.log(&pom) {
                    Ok(_) => {}
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

        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(start_break_options) => {
                let br3ak = Schedulable::new(pid, Kind::Break, start_break_options.duration.into());

                if verbose {
                    println!("Starting {}", br3ak);
                }

                match scheduler.run(br3ak, start_break_options.force) {
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
        SubCommands::Report(report_options) => match report_options.subcmd {
            ReportCommands::Day(day_options) => {
                use chrono::{Local, NaiveDate};
                use std::collections::HashMap;

                let date = match &day_options.date {
                    Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap_or_else(|e| {
                        eprintln!(
                            "Error: invalid date '{}': {}. Expected format: YYYY-MM-DD",
                            d, e
                        );
                        process::exit(1);
                    }),
                    None => Local::now().date_naive(),
                };

                let start_of_day = date
                    .and_hms_opt(0, 0, 0)
                    .and_then(|dt| dt.and_local_timezone(Local).earliest())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);
                let end_of_day = date
                    .and_hms_opt(23, 59, 59)
                    .and_then(|dt| dt.and_local_timezone(Local).earliest())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(i64::MAX);

                let repo = Repository::from_url(&db_url);

                let entries = match repo.entries_between(start_of_day, end_of_day) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                };

                let interrupt_logs = match repo.interrupts_between(start_of_day, end_of_day) {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                };

                let annotations = match repo.annotations_between(start_of_day, end_of_day) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                };

                // Group annotations by schedulable UUID for lookup
                let mut ann_by_uuid: HashMap<String, Vec<&Annotation>> = HashMap::new();
                for a in &annotations {
                    ann_by_uuid
                        .entry(a.schedulable_uuid.to_string())
                        .or_default()
                        .push(a);
                }

                // ── Print header ──────────────────────────────────
                let day_name = date.format("%A");
                println!("Report for {} ({})", date, day_name);
                println!("{}\n", "─".repeat(35));

                if entries.is_empty() {
                    println!("Nothing recorded for this day.");
                    process::exit(0);
                }

                // ── Entry list with annotations ────────────────────
                for entry in &entries {
                    let start = format_time(entry.started_at);
                    let end = if entry.finished_at != 0 {
                        format_time(entry.finished_at)
                    } else if entry.cancelled_at != 0 {
                        format_time(entry.cancelled_at)
                    } else {
                        "...".to_string()
                    };

                    let status_icon = match entry.status() {
                        Status::Finished => "\u{2713}",
                        Status::Cancelled => "\u{2717}",
                        Status::Active => "\u{2026}",
                        Status::Stale => "?",
                        Status::New => "?",
                    };

                    let interrupt_info = if entry.interruptions > 0 {
                        format!(" ({} int.)", entry.interruptions)
                    } else {
                        String::new()
                    };

                    println!(
                        " {:>5} - {:<5}  {:<9} ({:>2} min)  {}{}",
                        start,
                        end,
                        format!("{}", entry.kind),
                        entry.duration,
                        status_icon,
                        interrupt_info,
                    );

                    // Annotations for this entry
                    if let Some(notes) = ann_by_uuid.get(&entry.uuid.to_string()) {
                        for note in notes {
                            println!("    \u{2192} {}", note.body);
                        }
                    }
                }
                println!();

                // ── Summary metrics ────────────────────────────────
                let pomodori_completed = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Pomodoro && e.finished_at != 0)
                    .count();
                let pomodori_cancelled = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Pomodoro && e.cancelled_at != 0)
                    .count();
                let breaks_taken = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Break && e.finished_at != 0)
                    .count();
                let breaks_cancelled = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Break && e.cancelled_at != 0)
                    .count();

                let total_pomodori = pomodori_completed + pomodori_cancelled;
                let completion_rate = if total_pomodori > 0 {
                    (pomodori_completed as f64 / total_pomodori as f64 * 100.0) as u32
                } else {
                    0
                };

                // --- Interruption stats ---
                let total_interruptions: i64 = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Pomodoro)
                    .map(|e| e.interruptions)
                    .sum();

                let internal_count = interrupt_logs
                    .iter()
                    .filter(|l| l.kind == InterruptionKind::Internal)
                    .count();
                let external_count = interrupt_logs
                    .iter()
                    .filter(|l| l.kind == InterruptionKind::External)
                    .count();

                let avg_interruptions = if pomodori_completed > 0 {
                    total_interruptions as f64 / pomodori_completed as f64
                } else {
                    0.0
                };

                // --- Break ratio ---
                let break_ratio = if pomodori_completed > 0 {
                    breaks_taken as f64 / pomodori_completed as f64
                } else {
                    0.0
                };

                // --- Longest uninterrupted sequence (focus block) ---
                let max_focus_block = entries
                    .iter()
                    .filter(|e| e.kind == Kind::Pomodoro && e.finished_at != 0)
                    .fold((0usize, 0usize), |(max, current), pom| {
                        if pom.interruptions == 0 {
                            let new_current = current + 1;
                            (max.max(new_current), new_current)
                        } else {
                            (max, 0)
                        }
                    })
                    .0;

                println!(
                    "Pomodori    {} completed  \u{00b7}  {} cancelled  \u{00b7}  {}% completion rate",
                    pomodori_completed, pomodori_cancelled, completion_rate
                );
                println!(
                    "Breaks      {} taken      \u{00b7}  {} cancelled",
                    breaks_taken, breaks_cancelled
                );
                if pomodori_completed > 0 && breaks_taken > 0 {
                    let ratio_indicator = if (0.5..=2.0).contains(&break_ratio) {
                        "\u{2713}"
                    } else {
                        "\u{26a0}"
                    };
                    println!(
                        "Ratio       {:.1} break per pomodoro  {}",
                        break_ratio, ratio_indicator
                    );
                }
                println!();

                if max_focus_block > 1 {
                    println!(
                        "Longest focus block:  {} consecutive pomodori without interruption",
                        max_focus_block
                    );
                    println!();
                }

                println!("Interruptions");
                println!(
                    "  Total:      {} ({:.1} avg per pomodoro)",
                    total_interruptions, avg_interruptions
                );
                let total_logged = internal_count + external_count;
                if total_logged > 0 {
                    let internal_pct = (internal_count as f64 / total_logged as f64 * 100.0) as u32;
                    let external_pct = (external_count as f64 / total_logged as f64 * 100.0) as u32;
                    println!("  Internal:   {} ({}%)", internal_count, internal_pct);
                    println!("  External:   {} ({}%)", external_count, external_pct);
                } else if total_interruptions > 0 {
                    println!(
                        "  (Kind breakdown not available for interruptions recorded before the upgrade)"
                    );
                }
            }
            ReportCommands::Week(week_options) => {
                rustomato::report::print_week_report(
                    &Repository::from_url(&db_url),
                    week_options.date,
                );
            }
            ReportCommands::Interruptions(int_options) => {
                rustomato::report::print_interruptions_report(
                    &Repository::from_url(&db_url),
                    int_options.date,
                    int_options.days,
                );
            }
            ReportCommands::Month(month_options) => {
                rustomato::report::print_month_report(
                    &Repository::from_url(&db_url),
                    month_options.date,
                    month_options.months,
                );
            }
            ReportCommands::Last(last_options) => {
                rustomato::report::print_last_report(
                    &Repository::from_url(&db_url),
                    last_options.date,
                    last_options.days,
                );
            }
        },
        SubCommands::Completions(_) => unreachable!(),
    };
}

/// Format a Unix timestamp as "HH:MM".
fn format_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    if timestamp == 0 {
        return "N/A".to_string();
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| timestamp.to_string())
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
