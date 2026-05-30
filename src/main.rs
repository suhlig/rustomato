use clap::{CommandFactory, Parser, crate_version};
use clap_complete::{Shell, generate};
use rustomato::hooks;
use rustomato::persistence::Repository;
use rustomato::scheduling::{Scheduler, SchedulingError};
use rustomato::{InterruptionKind, Kind, Schedulable, Status, abbreviate_uuids, format_timestamp};
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
    subcmd: Option<SubCommands>,
}

#[derive(Parser)]
enum SubCommands {
    /// Initialize the rustomato root directory with sample hooks
    Init(InitCommand),
    Pomodoro(PomodoroCommand),
    Break(BreakCommand),
    Status(StatusCommand),
    /// List recent pomodori and breaks
    List(ListCommand),
    /// Show details of a specific pomodoro or break
    Show(ShowCommand),
    /// Generate a productivity report
    Report(ReportCommand),
    /// Display the man page
    Man(ManCommand),
    #[clap(hide = true)]
    Completions(CompletionsCommand),
}

/// Initialize rustomato
#[derive(Parser)]
struct InitCommand {}

/// Show the man page
#[derive(Parser)]
struct ManCommand {}

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
    Cancel(CancelPomodoro),
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

/// Cancel the current Pomodoro
#[derive(Parser)]
struct CancelPomodoro {}

/// Annotates a Pomodoro
#[derive(Parser)]
struct AnnotatePomodoro {
    /// The annotation text. Reads from STDIN if not provided.
    words: Vec<String>,

    /// Target: a UUID prefix, -1..-9 for recent finished pomodori, or a timestamp (HH:MM / RFC 3339)
    #[clap(short, long, value_name = "TARGET", allow_hyphen_values = true)]
    target: Option<String>,
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
    Cancel(CancelBreak),
}

/// Starts a break
#[derive(Parser)]
struct StartBreak {
    /// How many minutes this break should last (default depends on pomodoro count)
    #[clap(short, long, value_name = "DURATION")]
    duration: Option<u8>,

    /// Cancel whatever may currently be running before starting the break
    #[clap(short, long)]
    force: bool,
}

/// Cancel the current Break
#[derive(Parser)]
struct CancelBreak {}

/// Annotates a Break
#[derive(Parser)]
struct AnnotateBreak {
    /// The annotation text. Reads from STDIN if not provided.
    words: Vec<String>,

    /// Target: a UUID prefix, -1..-9 for recent finished pomodori, or a timestamp (HH:MM / RFC 3339)
    #[clap(short, long, value_name = "TARGET", allow_hyphen_values = true)]
    target: Option<String>,
}

/// Finishes the active Break
#[derive(Parser)]
struct FinishBreak {}

/// Report status
#[derive(Parser)]
struct StatusCommand {}

/// List recent pomodori and breaks
#[derive(Parser)]
struct ListCommand {
    /// Maximum number of entries to show
    #[clap(short, long, default_value = "10")]
    limit: u32,

    /// Omit the header and separator lines (useful for scripting)
    #[clap(long)]
    no_header: bool,
}

/// Show details of a specific pomodoro or break
#[derive(Parser)]
struct ShowCommand {
    /// UUID prefix, -1..-9 for recent finished pomodori, or a timestamp (HH:MM / RFC 3339)
    uuid: String,
}

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

    let subcmd = match opts.subcmd {
        Some(s) => s,
        None => {
            // No subcommand given, show help
            let mut cmd = Opts::command();
            cmd.print_help().unwrap();
            println!();
            process::exit(0);
        }
    };

    // The man and completions subcommands don't need a database, so handle them early.
    if let SubCommands::Man(_) = &subcmd {
        print!("{}", include_str!("../man/man1/rustomato.1"));
        return;
    }

    if let SubCommands::Completions(completions_options) = &subcmd {
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
        Ok(val) => {
            let root = PathBuf::from(val);
            if !root.exists() {
                std::fs::create_dir_all(root.as_path()).expect("creating the root directory");
            }
            root
        }
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
    if let SubCommands::Init(_) = &subcmd {
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

    match subcmd {
        SubCommands::Init(_) => unreachable!(), // handled above
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(ref opts) => cmd_pomodoro_start(&scheduler, opts, pid, verbose),
            PomodoroCommands::Interrupt(ref opts) => {
                cmd_pomodoro_interrupt(&scheduler, opts, verbose)
            }
            PomodoroCommands::Log(ref opts) => cmd_pomodoro_log(&scheduler, opts, verbose),
            PomodoroCommands::Annotate(ref opts) => {
                cmd_annotate(&scheduler, &opts.words, opts.target.as_deref(), verbose)
            }
            PomodoroCommands::Cancel(_) => cmd_cancel(&scheduler, verbose),
        },
        SubCommands::Status(_) => cmd_status(&db_url),
        SubCommands::List(ref opts) => cmd_list(&db_url, opts),
        SubCommands::Show(ref opts) => cmd_show(&db_url, opts),
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(ref opts) => cmd_break_start(&scheduler, opts, pid, verbose),
            BreakCommands::Annotate(ref opts) => {
                cmd_annotate(&scheduler, &opts.words, opts.target.as_deref(), verbose)
            }
            BreakCommands::Cancel(_) => cmd_cancel(&scheduler, verbose),
        },
        SubCommands::Report(report_options) => match report_options.subcmd {
            ReportCommands::Day(day_options) => {
                let repo = Repository::from_url(&db_url);
                rustomato::report::print_day_report(&repo, day_options.date);
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
        SubCommands::Man(_) => unreachable!(),
        SubCommands::Completions(_) => unreachable!(),
    };
}

// ── Command handlers ────────────────────────────────────────────

fn cmd_pomodoro_start(scheduler: &Scheduler, opts: &StartPomodoro, pid: u32, verbose: bool) {
    let pom = Schedulable::new(pid, Kind::Pomodoro, opts.duration.into());
    if verbose {
        println!("Starting {}", pom);
    }
    match scheduler.run(pom, opts.force) {
        Ok(completed_pom) => {
            if verbose {
                println!("\n{}", completed_pom);
            }
            match completed_pom.status() {
                Status::Cancelled => process::exit(1),
                Status::Finished => process::exit(0),
                _ => (),
            }
        }
        Err(err) => {
            match err {
                SchedulingError::AlreadyRunning(_) => eprintln!("Error: {}.", err),
                SchedulingError::HookRejected => process::exit(1),
                _ => eprintln!("Error: {}.", err),
            }
            process::exit(1);
        }
    }
}

fn cmd_pomodoro_interrupt(scheduler: &Scheduler, opts: &InterruptPomodoro, verbose: bool) {
    let kind = match InterruptionKind::from(&opts.kind) {
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

fn cmd_pomodoro_log(scheduler: &Scheduler, opts: &LogPomodoro, verbose: bool) {
    let (started_at, finished_at) = match (&opts.started_at, &opts.finished_at, opts.duration) {
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
            eprintln!("Error: at least one of --started-at or --finished-at is required.");
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

    if let Err(err) = scheduler.log(&pom) {
        eprintln!("Error: {}.", err);
        process::exit(1);
    }
}

fn cmd_annotate(scheduler: &Scheduler, words: &[String], target: Option<&str>, verbose: bool) {
    let text = annotation_text(words);
    if text.is_empty() {
        eprintln!("Error: annotation text is empty.");
        process::exit(1);
    }
    if verbose {
        println!("Annotating with '{}'", text);
    }
    let result = match target {
        Some(t) => scheduler.annotate_target(&text, t),
        None => scheduler.annotate(&text),
    };
    match result {
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

fn cmd_cancel(scheduler: &Scheduler, verbose: bool) {
    match scheduler.cancel() {
        Ok(schedulable) => {
            if verbose {
                println!("{}", schedulable);
            }
            match schedulable.kind {
                Kind::Pomodoro => process::exit(1),
                Kind::Break => process::exit(0),
            }
        }
        Err(err) => {
            eprintln!("Error: {}.", err);
            process::exit(1);
        }
    }
}

fn cmd_break_start(scheduler: &Scheduler, opts: &StartBreak, pid: u32, verbose: bool) {
    let duration = match opts.duration {
        Some(d) => d as i64,
        None => {
            let count = scheduler.repo().consecutive_pomodoro_count().unwrap_or(0);
            if count > 0 && count % 4 == 0 {
                eprintln!("Using 15-minute long break after {} pomodori", count);
                15
            } else {
                5
            }
        }
    };
    let br3ak = Schedulable::new(pid, Kind::Break, duration);
    if verbose {
        println!("Starting {}", br3ak);
    }
    match scheduler.run(br3ak, opts.force) {
        Ok(completed_break) => {
            if verbose {
                println!("\n{}", completed_break);
            }
            process::exit(0);
        }
        Err(err) => {
            match err {
                SchedulingError::AlreadyRunning(_) => eprintln!("Error: {}.", err),
                SchedulingError::HookRejected => process::exit(1),
                _ => eprintln!("Error: {}.", err),
            }
            process::exit(1);
        }
    }
}

fn cmd_status(db_url: &Url) {
    match Repository::from_url(db_url).active() {
        Ok(schedulable) => match schedulable {
            Some(existing) => println!("{}", existing),
            None => println!("Nothing active"),
        },
        Err(e) => eprintln!("{}", e),
    }
}

fn cmd_list(db_url: &Url, opts: &ListCommand) {
    if opts.limit == 0 {
        eprintln!("Error: --limit must be > 0.");
        process::exit(1);
    }

    let repo = Repository::from_url(db_url);
    let entries = match repo.list(opts.limit as i64) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: {}.", e);
            process::exit(1);
        }
    };

    if entries.is_empty() {
        println!("No entries found.");
        return;
    }

    let uuids: Vec<_> = entries.iter().map(|s| s.uuid).collect();
    let abbreviations = abbreviate_uuids(&uuids);
    let uuid_width = abbreviations.first().map(|s| s.len()).unwrap_or(6);
    let kind_width = entries
        .iter()
        .map(|s| s.kind.to_string().len())
        .max()
        .unwrap_or(8)
        .max(4);
    let started_width = 12;

    if !opts.no_header {
        // Header
        println!(
            "{:width$}  {:kind_width$}  {:started_width$}  Timeline",
            "UUID",
            "Kind",
            "Started",
            width = uuid_width.max(4),
            kind_width = kind_width,
            started_width = started_width
        );

        // Separator
        println!(
            "{:-<width$}  {:-<kind_width$}  {:-<started_width$}  ---------",
            "",
            "",
            "",
            width = uuid_width.max(4),
            kind_width = kind_width,
            started_width = started_width
        );
    }

    for (entry, abbrev) in entries.iter().zip(abbreviations.iter()) {
        let started = format_started(entry.started_at);
        let timeline = format_timeline(entry);
        println!(
            "{:width$}  {:kind_width$}  {:started_width$}  {}",
            abbrev,
            entry.kind.to_string(),
            started,
            timeline,
            width = uuid_width.max(4),
            kind_width = kind_width,
            started_width = started_width
        );
    }
}

/// Show detailed information about a single schedulable.
fn cmd_show(db_url: &Url, opts: &ShowCommand) {
    let repo = Repository::from_url(db_url);

    // Use the scheduler's resolve_target logic: UUID prefix, -N, or timestamp
    // Build a temporary scheduler with no hooks so we can use resolve_target
    let sched = Scheduler::new(
        repo,
        PathBuf::from("/"),
        false,
        true, // no-hooks
    );

    let schedulable = match sched.resolve_target(&opts.uuid) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}.", e);
            process::exit(1);
        }
    };

    let annotations = sched
        .repo()
        .annotations_for(schedulable.uuid)
        .unwrap_or_default();
    let interrupts = sched
        .repo()
        .interrupts_for(schedulable.uuid)
        .unwrap_or_default();

    let status_str = match schedulable.status() {
        Status::Active => "active",
        Status::Stale => "stale",
        Status::Finished => "finished",
        Status::Cancelled => "cancelled",
        Status::New => "new",
    };

    let duration_min = schedulable.duration;
    let started_str = format_timestamp(schedulable.started_at);
    let finished_str = if schedulable.finished_at != 0 {
        format_timestamp(schedulable.finished_at)
    } else if schedulable.cancelled_at != 0 {
        format_timestamp(schedulable.cancelled_at)
    } else {
        String::from("—")
    };

    // Compute elapsed duration for display
    let elapsed = if schedulable.finished_at != 0 {
        schedulable.finished_at - schedulable.started_at
    } else if schedulable.cancelled_at != 0 {
        schedulable.cancelled_at - schedulable.started_at
    } else {
        0
    };
    let elapsed_min = elapsed / 60;

    println!("  Kind: {}", schedulable.kind);
    println!("Status: {}", status_str);
    println!(
        "  When: {} → {} ({} min / planned {})",
        started_str, finished_str, elapsed_min, duration_min
    );
    println!("    ID: {}", schedulable.uuid);
    println!("    ");

    // Annotations
    println!("Annotations:");
    if annotations.is_empty() {
        println!("  (none)");
    } else {
        for a in &annotations {
            println!("  • {} ({})", a.body, format_timestamp(a.created_at));
        }
    }
    println!();

    // Interrupts
    println!("Interrupts:");
    if interrupts.is_empty() {
        println!("  (none)");
    } else {
        for i in &interrupts {
            println!(
                "  • {} ({})",
                i.kind.as_str(),
                format_timestamp(i.created_at)
            );
        }
    }
}

/// Format a started_at timestamp for the list view.
///
/// Shows:
/// - Today:          "HH:MM"       (e.g. "11:42")
/// - 1-6 days ago:   "Day HH:MM"   (e.g. "Sat 11:42")
/// - 7+ days ago:    "YYYY-MM-DD"  (e.g. "2026-05-23")
fn format_started(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};

    if timestamp == 0 {
        return "N/A".to_string();
    }

    let dt = match Local.timestamp_opt(timestamp, 0).single() {
        Some(dt) => dt,
        None => return timestamp.to_string(),
    };

    let today = Local::now().date_naive();
    let entry_date = dt.date_naive();
    let days_diff = (today - entry_date).num_days();

    if days_diff == 0 {
        dt.format("%H:%M").to_string()
    } else if days_diff <= 6 {
        dt.format("%a %H:%M").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

/// Build a human-readable timeline string for a schedulable.
fn format_timeline(s: &Schedulable) -> String {
    use chrono::Local;

    let elapsed_secs = match s.status() {
        rustomato::Status::Finished => s.finished_at - s.started_at,
        rustomato::Status::Cancelled => s.cancelled_at - s.started_at,
        rustomato::Status::Active | rustomato::Status::Stale => {
            Local::now().timestamp() - s.started_at
        }
        rustomato::Status::New => 0,
    };

    let minutes = elapsed_secs / 60;
    let seconds = elapsed_secs % 60;

    let duration_str = if minutes >= 1 {
        let noun = if minutes == 1 { "minute" } else { "minutes" };
        format!("{} {}", minutes, noun)
    } else {
        let noun = if seconds == 1 { "second" } else { "seconds" };
        format!("{} {}", seconds, noun)
    };

    let action = match s.status() {
        rustomato::Status::Finished => "finished",
        rustomato::Status::Cancelled => "cancelled",
        rustomato::Status::Active => "running",
        rustomato::Status::Stale => "stale",
        rustomato::Status::New => "unknown",
    };

    if action == "running" {
        if s.interruptions > 0 {
            let noun = if s.interruptions == 1 {
                "interruption"
            } else {
                "interruptions"
            };
            format!(
                "running for {} and {} {}",
                duration_str, s.interruptions, noun
            )
        } else {
            format!("running for {}", duration_str)
        }
    } else if action == "stale" {
        if s.interruptions > 0 {
            let noun = if s.interruptions == 1 {
                "interruption"
            } else {
                "interruptions"
            };
            format!(
                "stale after {} and {} {}",
                duration_str, s.interruptions, noun
            )
        } else {
            format!("stale after {}", duration_str)
        }
    } else {
        if s.interruptions > 0 {
            let noun = if s.interruptions == 1 {
                "interruption"
            } else {
                "interruptions"
            };
            format!(
                "{} after {} and {} {}",
                action, duration_str, s.interruptions, noun
            )
        } else {
            format!("{} after {}", action, duration_str)
        }
    }
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
