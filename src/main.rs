use clap::{Args, CommandFactory, Parser, crate_version};
use clap_complete::{Shell, generate};
use rustomato::hooks;
use rustomato::persistence::Repository;
use rustomato::scheduling::{Scheduler, SchedulingError};
use rustomato::{InterruptionKind, Kind, Schedulable, Status, abbreviate_uuids, format_timestamp};
use std::io;
use std::path::{Path, PathBuf};
use std::{env, process};

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
    /// Export entries as CSV for external analysis
    Export(ExportCommand),
    #[clap(hide = true)]
    Completions(CompletionsCommand),
}

/// Initialize rustomato
#[derive(Parser)]
struct InitCommand {}

/// Show the man page
#[derive(Parser)]
struct ManCommand {}

/// Export entries as CSV for external analysis
#[derive(Parser)]
struct ExportCommand {
    /// Start date (YYYY-MM-DD). Defaults to the earliest entry.
    #[clap(long, value_name = "DATE")]
    from: Option<String>,
    /// End date (YYYY-MM-DD). Defaults to now.
    #[clap(long, value_name = "DATE")]
    to: Option<String>,
}

/// Common `--target` / positional `-N` selection for commands that act on a
/// specific entry. The two are mutually exclusive.
#[derive(Args)]
struct TargetArg {
    /// Target: a UUID prefix, -1..-9 for recent entries, or a timestamp (HH:MM / RFC 3339)
    #[clap(short, long, value_name = "TARGET", allow_hyphen_values = true)]
    target: Option<String>,

    /// Shorthand: -1..-9 for recent entries. Conflicts with --target.
    #[clap(allow_hyphen_values = true, conflicts_with = "target")]
    index: Option<String>,
}

impl TargetArg {
    /// The explicitly selected target (`--target` or positional `-N`), if any.
    fn selected(&self) -> Option<&str> {
        self.target.as_deref().or(self.index.as_deref())
    }
}

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
    Log(LogCommand),
    Cancel(CancelPomodoro),
    Delete(DeletePomodoro),
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

    /// Cancel whatever may currently be running before starting the Pomodoro
    #[clap(short, long)]
    force: bool,
}

/// Marks a Pomodoro as interrupted
#[derive(Parser)]
struct InterruptPomodoro {
    /// Whether the interruption is internal (self-inflicted) or external (environmental)
    #[clap(
        short,
        long,
        value_enum,
        default_value_t = InterruptionKind::Internal,
        value_name = "KIND"
    )]
    kind: InterruptionKind,

    #[clap(flatten)]
    target: TargetArg,
}

/// Log an externally completed session
#[derive(Parser)]
struct LogCommand {
    /// When the session started (RFC 3339 / ISO 8601, HH:MM, or Unix timestamp)
    #[clap(long, value_name = "TIMESTAMP")]
    started_at: Option<String>,

    /// When the session finished (RFC 3339 / ISO 8601, HH:MM, or Unix timestamp)
    #[clap(long, value_name = "TIMESTAMP")]
    finished_at: Option<String>,

    /// Duration in minutes (default: 25 for pomodoro, 5 for break). Cannot be used when both --started-at and --finished-at are given.
    #[clap(short, long, value_name = "MINUTES")]
    duration: Option<u8>,
}

/// Cancel the current Pomodoro, or a specific one with --target.
#[derive(Parser)]
struct CancelPomodoro {
    #[clap(flatten)]
    target: TargetArg,
}

/// Annotates a Pomodoro
#[derive(Parser)]
struct AnnotatePomodoro {
    /// The annotation text. Reads from STDIN if not provided.
    #[clap(allow_hyphen_values = true)]
    words: Vec<String>,

    /// Target: a UUID prefix, -1..-9 for recent finished pomodori, or a timestamp (HH:MM / RFC 3339)
    #[clap(short, long, value_name = "TARGET", allow_hyphen_values = true)]
    target: Option<String>,
}

/// Work with a Break
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
    Log(LogCommand),
    Delete(DeleteBreak),
}

/// Starts a Break
#[derive(Parser)]
struct StartBreak {
    /// How many minutes this Break should last (default depends on pomodoro count)
    #[clap(short, long, value_name = "DURATION")]
    duration: Option<u8>,

    /// Cancel whatever may currently be running before starting the Break
    #[clap(short, long)]
    force: bool,
}

/// Deletes a past pomodoro.
#[derive(Parser)]
struct DeletePomodoro {
    #[clap(flatten)]
    target: TargetArg,
}

/// Deletes a past break.
#[derive(Parser)]
struct DeleteBreak {
    #[clap(flatten)]
    target: TargetArg,
}

/// Cancel the current Break, or a specific one with --target.
#[derive(Parser)]
struct CancelBreak {
    #[clap(flatten)]
    target: TargetArg,
}

/// Annotates a Break
#[derive(Parser)]
struct AnnotateBreak {
    /// The annotation text. Reads from STDIN if not provided.
    #[clap(allow_hyphen_values = true)]
    words: Vec<String>,

    /// Target: a UUID prefix, -1..-9 for recent finished breaks, or a timestamp (HH:MM / RFC 3339)
    #[clap(short, long, value_name = "TARGET", allow_hyphen_values = true)]
    target: Option<String>,
}

/// Report status
#[derive(Parser)]
struct StatusCommand {}

/// List recent pomodori and breaks
#[derive(Parser)]
struct ListCommand {
    /// Maximum number of entries to show
    #[clap(short, long, default_value = "10", value_parser = clap::value_parser!(u32).range(1..))]
    limit: u32,

    /// Omit the header and separator lines (useful for scripting)
    #[clap(long)]
    no_header: bool,
}

/// Show details of a specific pomodoro or break
#[derive(Parser)]
struct ShowCommand {
    /// UUID prefix, -1..-9 for recent entries, or a timestamp (HH:MM / RFC 3339)
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

/// An error that aborts the CLI invocation.
enum CliError {
    /// Print `Error: {0}.` to stderr, then exit 1.
    Failure(String),
    /// Exit 1 without printing (used when a before-hook already reported the failure).
    Silent,
}

impl From<SchedulingError> for CliError {
    fn from(e: SchedulingError) -> Self {
        match e {
            SchedulingError::HookRejected => CliError::Silent,
            other => CliError::Failure(other.to_string()),
        }
    }
}

/// Turn a command handler's result into a process exit.
fn finish(result: Result<i32, CliError>) -> ! {
    match result {
        Ok(code) => process::exit(code),
        Err(CliError::Failure(msg)) => {
            eprintln!("Error: {}.", msg);
            process::exit(1);
        }
        Err(CliError::Silent) => process::exit(1),
    }
}

/// Parse a timestamp CLI argument, reporting the flag that failed.
fn parse_opt_timestamp(value: &str, flag: &str) -> Result<i64, CliError> {
    rustomato::parse_timestamp(value).map_err(|e| CliError::Failure(format!("{} {}", e, flag)))
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
    match &subcmd {
        SubCommands::Man(_) => {
            let man = clap_mangen::Man::new(Opts::command());
            man.render(&mut io::stdout()).expect("writing the man page");
            return;
        }
        SubCommands::Completions(completions_options) => {
            let mut cmd = Opts::command();
            generate(
                completions_options.shell,
                &mut cmd,
                "rustomato",
                &mut io::stdout(),
            );
            return;
        }
        _ => {}
    }

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

    let db_path = match env::var("RUSTOMATO_DATABASE_URL") {
        Ok(val) => PathBuf::from(val),
        Err(_) => root.join("data.db"),
    };

    if verbose {
        println!("Using database {}", db_path.to_str().expect("converting"));
    }

    let repo = Repository::new(&db_path.to_string_lossy());
    let scheduler = Scheduler::new(repo, root, verbose, opts.no_hooks);
    let pid = process::id();

    let result = match subcmd {
        SubCommands::Init(_) => unreachable!(), // handled above
        SubCommands::Pomodoro(pomodoro_options) => match pomodoro_options.subcmd {
            PomodoroCommands::Start(ref opts) => cmd_pomodoro_start(&scheduler, opts, pid, verbose),
            PomodoroCommands::Interrupt(ref opts) => {
                cmd_pomodoro_interrupt(&scheduler, opts, verbose)
            }
            PomodoroCommands::Log(ref opts) => cmd_log(&scheduler, opts, Kind::Pomodoro, verbose),
            PomodoroCommands::Annotate(ref opts) => cmd_annotate(
                &scheduler,
                &opts.words,
                opts.target.as_deref(),
                Some(Kind::Pomodoro),
                verbose,
            ),
            PomodoroCommands::Cancel(ref opts) => {
                cmd_cancel(&scheduler, opts.target.selected(), verbose)
            }
            PomodoroCommands::Delete(ref opts) => {
                cmd_delete(&scheduler, opts.target.selected(), verbose)
            }
        },
        SubCommands::Status(_) => cmd_status(&db_path),
        SubCommands::List(ref opts) => cmd_list(&db_path, opts),
        SubCommands::Show(ref opts) => cmd_show(&db_path, opts),
        SubCommands::Break(break_options) => match break_options.subcmd {
            BreakCommands::Start(ref opts) => cmd_break_start(&scheduler, opts, pid, verbose),
            BreakCommands::Log(ref opts) => cmd_log(&scheduler, opts, Kind::Break, verbose),
            BreakCommands::Annotate(ref opts) => cmd_annotate(
                &scheduler,
                &opts.words,
                opts.target.as_deref(),
                Some(Kind::Break),
                verbose,
            ),
            BreakCommands::Cancel(ref opts) => {
                cmd_cancel(&scheduler, opts.target.selected(), verbose)
            }
            BreakCommands::Delete(ref opts) => {
                cmd_delete(&scheduler, opts.target.selected(), verbose)
            }
        },
        SubCommands::Report(report_options) => match report_options.subcmd {
            ReportCommands::Day(day_options) => {
                let repo = Repository::new(&db_path.to_string_lossy());
                rustomato::report::print_day_report(&repo, day_options.date);
                Ok(0)
            }
            ReportCommands::Week(week_options) => {
                rustomato::report::print_week_report(
                    &Repository::new(&db_path.to_string_lossy()),
                    week_options.date,
                );
                Ok(0)
            }
            ReportCommands::Interruptions(int_options) => {
                rustomato::report::print_interruptions_report(
                    &Repository::new(&db_path.to_string_lossy()),
                    int_options.date,
                    int_options.days,
                );
                Ok(0)
            }
            ReportCommands::Month(month_options) => {
                rustomato::report::print_month_report(
                    &Repository::new(&db_path.to_string_lossy()),
                    month_options.date,
                    month_options.months,
                );
                Ok(0)
            }
            ReportCommands::Last(last_options) => {
                rustomato::report::print_last_report(
                    &Repository::new(&db_path.to_string_lossy()),
                    last_options.date,
                    last_options.days,
                );
                Ok(0)
            }
        },
        SubCommands::Export(ref opts) => {
            let repo = Repository::new(&db_path.to_string_lossy());
            rustomato::export::cmd_export(&repo, opts.from.as_deref(), opts.to.as_deref());
            Ok(0)
        }
        SubCommands::Man(_) => unreachable!(),
        SubCommands::Completions(_) => unreachable!(),
    };

    finish(result);
}

// ── Command handlers ────────────────────────────────────────────

fn cmd_pomodoro_start(
    scheduler: &Scheduler,
    opts: &StartPomodoro,
    pid: u32,
    verbose: bool,
) -> Result<i32, CliError> {
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
                Status::Cancelled => Ok(1),
                Status::Finished => Ok(0),
                _ => Ok(0),
            }
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_pomodoro_interrupt(
    scheduler: &Scheduler,
    opts: &InterruptPomodoro,
    verbose: bool,
) -> Result<i32, CliError> {
    let result = match opts.target.selected() {
        Some(t) => scheduler.interrupt_target(opts.kind, t),
        None => scheduler.interrupt(opts.kind),
    };
    match result {
        Ok(interrupted) => {
            if verbose {
                println!("{}", interrupted);
            }
            Ok(0)
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_log(
    scheduler: &Scheduler,
    opts: &LogCommand,
    kind: Kind,
    verbose: bool,
) -> Result<i32, CliError> {
    let default_duration: i64 = match kind {
        Kind::Pomodoro => 25,
        Kind::Break => 5,
    };

    let (started_at, finished_at) = match (&opts.started_at, &opts.finished_at, opts.duration) {
        (Some(s), None, dur) => {
            let dur = dur.map_or(default_duration, i64::from);
            let started_at = parse_opt_timestamp(s, "--started-at")?;
            (started_at, started_at + dur * 60)
        }
        (None, Some(f), dur) => {
            let dur = dur.map_or(default_duration, i64::from);
            let finished_at = parse_opt_timestamp(f, "--finished-at")?;
            (finished_at - dur * 60, finished_at)
        }
        (Some(s), Some(f), None) => {
            let started_at = parse_opt_timestamp(s, "--started-at")?;
            let finished_at = parse_opt_timestamp(f, "--finished-at")?;
            (started_at, finished_at)
        }
        (Some(_), Some(_), Some(_)) => {
            return Err(CliError::Failure(
                "cannot specify --duration when both --started-at and --finished-at are given."
                    .to_string(),
            ));
        }
        (None, None, _) => {
            return Err(CliError::Failure(
                "at least one of --started-at or --finished-at is required.".to_string(),
            ));
        }
    };

    if finished_at < started_at {
        return Err(CliError::Failure(
            "--finished-at must be after --started-at.".to_string(),
        ));
    }

    let actual_duration = (finished_at - started_at) / 60;
    if verbose {
        println!(
            "Logging externally completed {} ({} min)",
            kind, actual_duration
        );
    }

    let mut schedulable = Schedulable::new(0, kind, actual_duration);
    schedulable.started_at = started_at;
    schedulable.finished_at = finished_at;

    scheduler.log(&schedulable)?;
    Ok(0)
}

fn cmd_annotate(
    scheduler: &Scheduler,
    words: &[String],
    target: Option<&str>,
    kind: Option<Kind>,
    verbose: bool,
) -> Result<i32, CliError> {
    // If no explicit --target, check if the first word is a negative-index
    // shorthand (-1..=-9) and use it as the target.
    let (resolved_target, words_for_text): (Option<String>, &[String]) =
        match (target, words.first()) {
            (Some(_), Some(w))
                if w.starts_with('-')
                    && w.len() > 1
                    && w[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                return Err(CliError::Failure(
                    "cannot use both --target and a positional index.".to_string(),
                ));
            }
            (None, Some(w))
                if w.starts_with('-')
                    && w.len() > 1
                    && w[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                (Some(w.clone()), &words[1..])
            }
            _ => (target.map(|s| s.to_string()), words),
        };
    let text = annotation_text(words_for_text);
    if text.is_empty() {
        return Err(CliError::Failure("annotation text is empty.".to_string()));
    }
    if verbose {
        println!("Annotating with '{}'", text);
    }
    let result = match resolved_target.as_deref() {
        Some(t) => scheduler.annotate_target(&text, t, kind),
        None => match kind {
            Some(k) => scheduler.annotate_for_kind(&text, k),
            None => scheduler.annotate(&text),
        },
    };
    match result {
        Ok(annotation) => {
            if verbose {
                println!("Annotated {}", annotation.body);
            }
            Ok(0)
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_cancel(scheduler: &Scheduler, target: Option<&str>, verbose: bool) -> Result<i32, CliError> {
    let result = match target {
        Some(t) => scheduler.cancel_target(t),
        None => scheduler.cancel(),
    };
    match result {
        Ok(schedulable) => {
            if verbose {
                println!("{}", schedulable);
            }
            match schedulable.kind {
                Kind::Pomodoro => Ok(1),
                Kind::Break => Ok(0),
            }
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_delete(scheduler: &Scheduler, target: Option<&str>, verbose: bool) -> Result<i32, CliError> {
    let Some(t) = target else {
        return Err(CliError::Failure(
            "--target or a positional index (-1..-9) is required for delete.".to_string(),
        ));
    };
    match scheduler.delete_target(t) {
        Ok(schedulable) => {
            if verbose {
                println!("Deleted {}", schedulable);
            }
            Ok(0)
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_break_start(
    scheduler: &Scheduler,
    opts: &StartBreak,
    pid: u32,
    verbose: bool,
) -> Result<i32, CliError> {
    let duration = match opts.duration {
        Some(d) => d as i64,
        None => {
            let count = scheduler.repo().consecutive_pomodoro_count().unwrap_or(0);
            if count > 0 && count % 4 == 0 {
                if verbose {
                    eprintln!("Using 15-minute long break after {} pomodori", count);
                }
                15
            } else {
                5
            }
        }
    };
    let bk = Schedulable::new(pid, Kind::Break, duration);
    if verbose {
        println!("Starting {}", bk);
    }
    match scheduler.run(bk, opts.force) {
        Ok(completed_break) => {
            if verbose {
                println!("\n{}", completed_break);
            }
            Ok(0)
        }
        Err(err) => Err(err.into()),
    }
}

fn cmd_status(db_path: &Path) -> Result<i32, CliError> {
    match Repository::new(&db_path.to_string_lossy()).active() {
        Ok(schedulable) => match schedulable {
            Some(existing) => println!("{}", existing),
            None => println!("Nothing active"),
        },
        Err(e) => return Err(CliError::Failure(e.to_string())),
    }
    Ok(0)
}

fn cmd_list(db_path: &Path, opts: &ListCommand) -> Result<i32, CliError> {
    let repo = Repository::new(&db_path.to_string_lossy());
    let entries = repo
        .list(opts.limit as i64)
        .map_err(|e| CliError::Failure(e.to_string()))?;

    if entries.is_empty() {
        println!("No entries found.");
        return Ok(0);
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

    Ok(0)
}

/// Show detailed information about a single schedulable.
fn cmd_show(db_path: &Path, opts: &ShowCommand) -> Result<i32, CliError> {
    let repo = Repository::new(&db_path.to_string_lossy());

    let schedulable =
        rustomato::scheduling::resolve_target(&repo, &opts.uuid, None).map_err(CliError::from)?;

    let annotations = repo
        .annotations_for(schedulable.uuid)
        .map_err(|e| CliError::Failure(e.to_string()))?;
    let interrupts = repo
        .interrupts_for(schedulable.uuid)
        .map_err(|e| CliError::Failure(e.to_string()))?;

    let status_str = schedulable.status().as_str();

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

    println!("Kind:   {}", schedulable.kind);
    println!("Status: {}", status_str);
    println!(
        "When:   {} → {} ({} min / planned {})",
        started_str, finished_str, elapsed_min, duration_min
    );
    println!("ID:     {}", schedulable.uuid);
    println!();

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

    Ok(0)
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

    let interruptions = if s.interruptions > 0 {
        format!(
            " and {} {}",
            s.interruptions,
            rustomato::interruption_noun(s.interruptions)
        )
    } else {
        String::new()
    };

    if action == "running" {
        format!("running for {}{}", duration_str, interruptions)
    } else if action == "stale" {
        format!("stale after {}{}", duration_str, interruptions)
    } else {
        format!("{} after {}{}", action, duration_str, interruptions)
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
