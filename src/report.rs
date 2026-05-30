use crate::persistence::Repository;
use crate::{InterruptLog, InterruptionKind, Kind, Schedulable};
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use std::collections::BTreeMap;

// ── Data structures ───────────────────────────────────────────

#[derive(Debug, Default)]
#[allow(dead_code)]
struct DayStats {
    date: NaiveDate,
    pomodori_completed: usize,
    pomodori_cancelled: usize,
    breaks_taken: usize,
    breaks_cancelled: usize,
    interruptions: i64,
    internal_interruptions: usize,
    external_interruptions: usize,
}

#[derive(Debug, Default)]
struct AggregateStats {
    completed: usize,
    cancelled: usize,
    completion_rate: u32,
    breaks_taken: usize,
    breaks_cancelled: usize,
    total_interruptions: i64,
    internal_count: usize,
    external_count: usize,
    avg_interruptions: f64,
    max_focus_block: usize,
    break_ratio: f64,
}

// ── Report builder ─────────────────────────────────────────────

/// A simple text report accumulator that centralises formatting helpers.
struct Report {
    buf: String,
}

impl Report {
    fn new() -> Self {
        Self { buf: String::new() }
    }

    fn line(&mut self, text: impl std::fmt::Display) {
        use std::fmt::Write;
        let _ = writeln!(self.buf, "{}", text);
    }

    fn blank(&mut self) {
        self.buf.push('\n');
    }

    fn separator(&mut self, width: usize) {
        self.buf.push_str(&"\u{2500}".repeat(width));
        self.buf.push('\n');
    }

    /// Indented line (2 spaces prefix).
    fn indent(&mut self, text: impl std::fmt::Display) {
        use std::fmt::Write;
        let _ = writeln!(self.buf, "  {}", text);
    }

    fn into_string(self) -> String {
        self.buf
    }
}

/// Ratio indicator — returns a checkmark or warning emoji.
fn ratio_indicator(ratio: f64) -> &'static str {
    if (0.5..=2.0).contains(&ratio) {
        "\u{2713}"
    } else {
        "\u{26a0}"
    }
}

/// Write the standard summary metrics block (pomodori, breaks, ratio, focus block, active days).
/// All metrics lines are indented by 2 spaces.
fn write_metrics(
    report: &mut Report,
    agg: &AggregateStats,
    prev_rate: Option<u32>,
    active_days: Option<(u32, u32, u32)>,
) {
    let prev_str = prev_rate
        .filter(|_| agg.completed > 0 || agg.cancelled > 0)
        .map(|r| format!(" (prev: {}%)", r))
        .unwrap_or_default();

    report.indent(format_args!(
        "Pomodori:     {} completed \u{00b7} {} cancelled  \u{00b7}  {}% completion rate{}",
        agg.completed, agg.cancelled, agg.completion_rate, prev_str
    ));
    report.indent(format_args!(
        "Breaks:       {} taken \u{00b7} {} cancelled",
        agg.breaks_taken, agg.breaks_cancelled
    ));

    if agg.completed > 0 && agg.breaks_taken > 0 {
        report.indent(format_args!(
            "Ratio:        {:.1} break per pomodoro  {}",
            agg.break_ratio,
            ratio_indicator(agg.break_ratio)
        ));
    }

    if agg.max_focus_block > 0 {
        report.indent(format_args!(
            "Focus block:  {} consecutive pomodori without interruption",
            agg.max_focus_block
        ));
    }

    if let Some((active_count, total_days, streak)) = active_days {
        let day = if streak == 1 { "day" } else { "days" };
        report.indent(format_args!(
            "Active days:  {} of {} ({:.0}%)  \u{00b7}  Best streak: {} {}",
            active_count,
            total_days,
            active_count as f64 / total_days as f64 * 100.0,
            streak,
            day
        ));
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn parse_date_or_today(date: Option<String>) -> NaiveDate {
    match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d").unwrap_or_else(|e| {
            eprintln!(
                "Error: invalid date '{}': {}. Expected format: YYYY-MM-DD",
                d, e
            );
            std::process::exit(1);
        }),
        None => Local::now().date_naive(),
    }
}

/// Returns (start_of_day_ts, end_of_day_ts) for a given date in local timezone.
fn day_bounds(date: NaiveDate) -> (i64, i64) {
    let start = date
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(Local).earliest())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    let end = date
        .and_hms_opt(23, 59, 59)
        .and_then(|dt| dt.and_local_timezone(Local).earliest())
        .map(|dt| dt.timestamp())
        .unwrap_or(i64::MAX);
    (start, end)
}

/// Fetch entries and interrupt logs for a time range, exiting on error.
fn fetch_data(repo: &Repository, start: i64, end: i64) -> (Vec<Schedulable>, Vec<InterruptLog>) {
    let entries = repo.entries_between(start, end).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    let interrupts = repo.interrupts_between(start, end).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    (entries, interrupts)
}

// ── Computation ──────────────────────────────────────────────

fn compute_aggregate(entries: &[Schedulable], interrupts: &[InterruptLog]) -> AggregateStats {
    let completed = entries
        .iter()
        .filter(|e| e.kind == Kind::Pomodoro && e.finished_at != 0)
        .count();
    let cancelled = entries
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

    let total = completed + cancelled;
    let completion_rate = if total > 0 {
        (completed as f64 / total as f64 * 100.0) as u32
    } else {
        0
    };

    let total_interruptions: i64 = entries
        .iter()
        .filter(|e| e.kind == Kind::Pomodoro)
        .map(|e| e.interruptions)
        .sum();

    let internal_count = interrupts
        .iter()
        .filter(|l| l.kind == InterruptionKind::Internal)
        .count();
    let external_count = interrupts
        .iter()
        .filter(|l| l.kind == InterruptionKind::External)
        .count();

    let avg_interruptions = if completed > 0 {
        total_interruptions as f64 / completed as f64
    } else {
        0.0
    };

    let break_ratio = if completed > 0 {
        breaks_taken as f64 / completed as f64
    } else {
        0.0
    };

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

    AggregateStats {
        completed,
        cancelled,
        completion_rate,
        breaks_taken,
        breaks_cancelled,
        total_interruptions,
        internal_count,
        external_count,
        avg_interruptions,
        max_focus_block,
        break_ratio,
    }
}

fn compute_day_stats(
    entries: &[Schedulable],
    interrupts: &[InterruptLog],
    monday: NaiveDate,
    sunday: NaiveDate,
) -> Vec<DayStats> {
    let mut days = Vec::new();
    let mut current = monday;
    while current <= sunday {
        let (start, end) = day_bounds(current);

        let day_entries: Vec<&Schedulable> = entries
            .iter()
            .filter(|e| e.started_at >= start && e.started_at <= end)
            .collect();
        let day_interrupts: Vec<&InterruptLog> = interrupts
            .iter()
            .filter(|l| l.created_at >= start && l.created_at <= end)
            .collect();

        let pomodori_completed = day_entries
            .iter()
            .filter(|e| e.kind == Kind::Pomodoro && e.finished_at != 0)
            .count();
        let pomodori_cancelled = day_entries
            .iter()
            .filter(|e| e.kind == Kind::Pomodoro && e.cancelled_at != 0)
            .count();
        let breaks_taken = day_entries
            .iter()
            .filter(|e| e.kind == Kind::Break && e.finished_at != 0)
            .count();
        let breaks_cancelled = day_entries
            .iter()
            .filter(|e| e.kind == Kind::Break && e.cancelled_at != 0)
            .count();
        let interruptions: i64 = day_entries
            .iter()
            .filter(|e| e.kind == Kind::Pomodoro)
            .map(|e| e.interruptions)
            .sum();
        let internal_interruptions = day_interrupts
            .iter()
            .filter(|l| l.kind == InterruptionKind::Internal)
            .count();
        let external_interruptions = day_interrupts
            .iter()
            .filter(|l| l.kind == InterruptionKind::External)
            .count();

        days.push(DayStats {
            date: current,
            pomodori_completed,
            pomodori_cancelled,
            breaks_taken,
            breaks_cancelled,
            interruptions,
            internal_interruptions,
            external_interruptions,
        });

        current += Duration::days(1);
    }
    days
}

/// Print the standard interruption summary block.
fn print_interruption_summary(
    report: &mut Report,
    agg: &AggregateStats,
    prev_label: Option<&str>,
    prev_agg: Option<&AggregateStats>,
) {
    report.line("Interruptions:");
    report.indent(format_args!(
        "Total:        {} ({:.1} avg per pomodoro)",
        agg.total_interruptions, agg.avg_interruptions
    ));
    let total_logged = agg.internal_count + agg.external_count;
    if total_logged > 0 {
        let internal_pct = (agg.internal_count as f64 / total_logged as f64 * 100.0) as u32;
        let external_pct = (agg.external_count as f64 / total_logged as f64 * 100.0) as u32;
        report.indent(format_args!(
            "Internal:     {} ({}%)",
            agg.internal_count, internal_pct
        ));
        report.indent(format_args!(
            "External:     {} ({}%)",
            agg.external_count, external_pct
        ));

        if let (Some(label), Some(prev)) = (prev_label, prev_agg) {
            let prev_total = prev.internal_count + prev.external_count;
            if prev_total > 0 {
                let prev_internal_pct =
                    (prev.internal_count as f64 / prev_total as f64 * 100.0) as u32;
                report.indent(format_args!(
                    "({}: {} internal \u{00b7} {} external, {}% internal)",
                    label, prev.internal_count, prev.external_count, prev_internal_pct
                ));
            }
        }
    } else if agg.total_interruptions > 0 {
        report
            .indent("(Kind breakdown not available for interruptions recorded before the upgrade)");
    }
    report.blank();
}

// ── Hints ────────────────────────────────────────────────────

fn check_pattern_hints(
    day_stats: &[DayStats],
    week: &AggregateStats,
    prev_week: &AggregateStats,
) -> Vec<String> {
    let mut hints = Vec::new();

    // Weekdays (Mon-Fri) with zero completed pomodori
    let zero_days = day_stats
        .iter()
        .filter(|d| d.date.weekday().num_days_from_monday() < 5 && d.pomodori_completed == 0)
        .count();
    if zero_days >= 3 {
        hints.push(format!(
            "⚠ You had {} weekdays with no completed pomodori. Consider reviewing your weekly schedule.",
            zero_days
        ));
    }

    // Low completion rate
    if week.completion_rate < 70 && week.completed + week.cancelled >= 3 {
        hints.push(format!(
            "⚠ Low completion rate ({}%). Try shorter pomodori or reviewing what's causing cancellations.",
            week.completion_rate
        ));
    }

    // No focus blocks
    if week.max_focus_block <= 1 && week.total_interruptions > 0 {
        hints.push("⚠ No consecutive uninterrupted pomodori. Consider silencing notifications and using a 'do not disturb' signal.".to_string());
    }

    // Break ratio too low (only if we have enough data to judge)
    if week.break_ratio > 0.0 && week.break_ratio < 0.3 && week.completed >= 3 {
        hints.push(
            "⚠ Few breaks relative to pomodori. Skipping breaks reduces cognitive performance over the day."
                .to_string(),
        );
    }

    // Interruption kind dominance
    let total_ilog = week.internal_count + week.external_count;
    if total_ilog > 0 {
        let internal_pct = week.internal_count as f64 / total_ilog as f64 * 100.0;
        if internal_pct > 70.0 {
            hints.push(
                "💡 Most interruptions are internal. Consider a 'parking lot' notepad to capture distracting thoughts during pomodori."
                    .to_string(),
            );
        } else if internal_pct < 30.0 {
            hints.push(
                "💡 Most interruptions are external. Can you negotiate focused blocks or use a status signal?"
                    .to_string(),
            );
        }
    }

    // Week-over-week trends
    if prev_week.completed > 0 {
        if week.completed < prev_week.completed && week.completion_rate <= prev_week.completion_rate
        {
            hints.push(
                "📉 Both completed count and completion rate declined this week. Consider whether workload has changed."
                    .to_string(),
            );
        } else if week.completed > prev_week.completed
            && week.completion_rate >= prev_week.completion_rate
        {
            hints.push(
                "📈 Completed more pomodori with maintained or improved completion rate. Great progress!"
                    .to_string(),
            );
        }

        if week.internal_count < prev_week.internal_count && prev_week.internal_count > 0 {
            hints.push(format!(
                "✅ Internal interruptions are decreasing ({} this week vs {} last week). Your focus discipline is improving.",
                week.internal_count, prev_week.internal_count
            ));
        }
    }

    hints
}

// ── Weekly report ────────────────────────────────────────────

/// Print a weekly productivity report covering the ISO week containing the given
/// date (defaults to today), with day-by-day breakdown, week-over-week comparison,
/// best/worst day, and actionable hints.
pub fn print_week_report(repo: &Repository, date: Option<String>) {
    let mut rpt = Report::new();

    let date = parse_date_or_today(date);
    let weekday = date.weekday().num_days_from_monday(); // Mon=0 … Sun=6
    let monday = date - Duration::days(weekday as i64);
    let sunday = monday + Duration::days(6);

    let prev_monday = monday - Duration::days(7);
    let prev_sunday = sunday - Duration::days(7);

    let (this_start, this_end) = (day_bounds(monday).0, day_bounds(sunday).1);
    let (prev_start, prev_end) = (day_bounds(prev_monday).0, day_bounds(prev_sunday).1);

    let (this_entries, this_interrupts) = fetch_data(repo, this_start, this_end);
    let (prev_entries, prev_interrupts) = fetch_data(repo, prev_start, prev_end);

    let day_stats = compute_day_stats(&this_entries, &this_interrupts, monday, sunday);
    let week = compute_aggregate(&this_entries, &this_interrupts);
    let prev_week = compute_aggregate(&prev_entries, &prev_interrupts);

    // Best / worst day
    let best_day = day_stats
        .iter()
        .filter(|d| d.pomodori_completed > 0)
        .max_by_key(|d| d.pomodori_completed);
    let worst_day = day_stats
        .iter()
        .filter(|d| d.pomodori_completed + d.pomodori_cancelled > 0)
        .min_by_key(|d| (d.pomodori_completed as i64) - (d.pomodori_cancelled as i64));

    // ── Header ────────────────────────────────────────────
    rpt.blank();
    rpt.line(format_args!(
        "Weekly Report: {} \u{2013} {}",
        monday.format("%b %d"),
        sunday.format("%b %d, %Y")
    ));
    if prev_week.completed > 0 || prev_week.cancelled > 0 || prev_week.breaks_taken > 0 {
        rpt.line(format_args!(
            "(vs week of {} \u{2013} {})",
            prev_monday.format("%b %d"),
            prev_sunday.format("%b %d")
        ));
    }
    rpt.separator(52);
    rpt.blank();

    // ── Day-by-day table ──────────────────────────────────
    rpt.line("Day-by-day breakdown:");
    rpt.indent("Day       Done   Canc  Brk \u{25bc}  Brk \u{2717}   Interr.");
    rpt.indent("\u{2500}".repeat(50));
    let has_any = day_stats
        .iter()
        .any(|d| d.pomodori_completed > 0 || d.pomodori_cancelled > 0 || d.breaks_taken > 0);
    if !has_any {
        rpt.indent("(nothing recorded this week)");
    } else {
        for ds in &day_stats {
            let day_name = ds.date.format("%a");
            let star = if Some(ds.date) == best_day.map(|d| d.date) {
                " \u{2605}"
            } else if Some(ds.date) == worst_day.map(|d| d.date) {
                " \u{2297}"
            } else {
                "  "
            };
            rpt.indent(format_args!(
                "{:6}{} {:>4}  {:>4}  {:>4}  {:>4}  {:>7}",
                day_name,
                star,
                ds.pomodori_completed,
                ds.pomodori_cancelled,
                ds.breaks_taken,
                ds.breaks_cancelled,
                ds.interruptions,
            ));
        }
    }
    rpt.blank();

    // ── Weekly summary ────────────────────────────────────
    rpt.line("Weekly summary:");
    if week.completed == 0 && week.cancelled == 0 && week.breaks_taken == 0 {
        rpt.indent("No pomodori or breaks recorded this week.");
        rpt.blank();
        print!("{}", rpt.into_string());
        return;
    }

    write_metrics(&mut rpt, &week, Some(prev_week.completion_rate), None);
    rpt.blank();

    // ── Interruptions ─────────────────────────────────────
    print_interruption_summary(&mut rpt, &week, Some("prev week"), Some(&prev_week));

    // ── Best / worst day ──────────────────────────────────
    if let Some(best) = best_day {
        rpt.line(format_args!(
            "\u{2605}  Best day: {} ({} completed)",
            best.date.format("%A"),
            best.pomodori_completed
        ));
    }
    if let Some(worst) = worst_day
        && (worst.pomodori_completed > 0 || worst.pomodori_cancelled > 0)
    {
        rpt.line(format_args!(
            "\u{2297}  Worst day: {} ({} completed, {} cancelled)",
            worst.date.format("%A"),
            worst.pomodori_completed,
            worst.pomodori_cancelled
        ));
    }
    rpt.blank();

    // ── Hints ─────────────────────────────────────────────
    let hints = check_pattern_hints(&day_stats, &week, &prev_week);
    if !hints.is_empty() {
        rpt.line("Insights:");
        for hint in &hints {
            rpt.indent(hint);
        }
        rpt.blank();
    }

    print!("{}", rpt.into_string());
}

// ── Interruption patterns report ────────────────────────────

/// Print an interruption pattern report covering the last N days from the given
/// date, broken down by hour of day and day of week with internal/external split.
pub fn print_interruptions_report(repo: &Repository, date: Option<String>, days: u32) {
    let mut rpt = Report::new();

    let date = parse_date_or_today(date);
    let start_date = date - Duration::days(days as i64 - 1);
    let (start, end) = (day_bounds(start_date).0, day_bounds(date).1);

    let (entries, interrupts) = fetch_data(repo, start, end);

    let period_label = if days == 1 {
        format!("{}", date.format("%b %d, %Y"))
    } else {
        format!(
            "{} – {} (last {} days)",
            start_date.format("%b %d"),
            date.format("%b %d, %Y"),
            days
        )
    };

    rpt.blank();
    rpt.line(format_args!("Interruption Patterns: {}", period_label));
    rpt.separator(52);
    rpt.blank();

    if interrupts.is_empty() {
        let total_interruptions: i64 = entries
            .iter()
            .filter(|e| e.kind == Kind::Pomodoro)
            .map(|e| e.interruptions)
            .sum();
        if total_interruptions > 0 {
            rpt.indent(format_args!("{} interruption(s) recorded via counter in this period, but the interrupt log\n  (which provides kind/hour/day breakdown) is empty. Interruptions recorded\n  before the upgrade are not included in this report.", total_interruptions));
        } else {
            rpt.indent("No interruptions recorded in this period.");
        }
        rpt.blank();
        print!("{}", rpt.into_string());
        return;
    }

    // ── Group by hour of day ──────────────────────────────
    let mut by_hour: BTreeMap<u32, (usize, usize, usize)> = BTreeMap::new();
    // ── Group by day of week ──────────────────────────────
    let mut by_weekday: BTreeMap<u32, (usize, usize, usize)> = BTreeMap::new();

    for interrupt in &interrupts {
        let Some(utc_dt) = chrono::DateTime::from_timestamp(interrupt.created_at, 0) else {
            continue;
        };
        let dt = utc_dt.with_timezone(&Local);
        let hour = dt.hour();
        let weekday = dt.date_naive().weekday().num_days_from_monday();

        let entry_h = by_hour.entry(hour).or_insert((0, 0, 0));
        entry_h.0 += 1;
        match interrupt.kind {
            InterruptionKind::Internal => entry_h.1 += 1,
            InterruptionKind::External => entry_h.2 += 1,
        }

        let entry_w = by_weekday.entry(weekday).or_insert((0, 0, 0));
        entry_w.0 += 1;
        match interrupt.kind {
            InterruptionKind::Internal => entry_w.1 += 1,
            InterruptionKind::External => entry_w.2 += 1,
        }
    }

    let max_hour_total = by_hour.values().map(|(t, _, _)| *t).max().unwrap_or(0);
    let max_wd_total = by_weekday.values().map(|(t, _, _)| *t).max().unwrap_or(0);

    // ── Hourly breakdown ──────────────────────────────────
    rpt.line("By hour of day:");
    rpt.indent("Hour      Total  Internal  External");
    rpt.indent(format_args!("{}", "─".repeat(40)));
    for hour in 0..24 {
        let (total, internal, external) = by_hour.get(&hour).copied().unwrap_or((0, 0, 0));
        if total > 0 {
            let marker = if total == max_hour_total && total > 0 {
                "  ⚠"
            } else {
                "   "
            };
            rpt.indent(format_args!(
                "{:02}:00    {:>5}  {:>8}  {:>8}{}",
                hour, total, internal, external, marker
            ));
        }
    }
    rpt.blank();

    // ── Day-of-week breakdown ─────────────────────────────
    let weekday_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    rpt.line("By day of week:");
    rpt.indent("Day       Total  Internal  External");
    rpt.indent(format_args!("{}", "─".repeat(40)));
    for wd in 0..7 {
        let (total, internal, external) = by_weekday.get(&wd).copied().unwrap_or((0, 0, 0));
        if total > 0 {
            let marker = if total == max_wd_total && total > 0 {
                "  ⚠"
            } else {
                "   "
            };
            rpt.indent(format_args!(
                "{:6}  {:>5}  {:>8}  {:>8}{}",
                weekday_names[wd as usize], total, internal, external, marker
            ));
        }
    }
    rpt.blank();

    // ── Summary stats ─────────────────────────────────────
    let total_count: usize = interrupts.len();
    let internal_count = interrupts
        .iter()
        .filter(|l| l.kind == InterruptionKind::Internal)
        .count();
    let external_count = total_count - internal_count;
    let internal_pct = (internal_count as f64 / total_count as f64 * 100.0) as u32;
    let external_pct = (external_count as f64 / total_count as f64 * 100.0) as u32;

    rpt.line(format_args!(
        "Total: {} interruptions ({} internal · {} external, {}% / {}%)",
        total_count, internal_count, external_count, internal_pct, external_pct
    ));
    rpt.blank();

    print!("{}", rpt.into_string());
}

// ── Monthly report ───────────────────────────────────────────

/// Parse a date string that may be `YYYY-MM` (defaults to the 1st) or
/// `YYYY-MM-DD`, or defaults to today.
fn parse_month_date(date: Option<String>) -> NaiveDate {
    match date {
        Some(d) => {
            // Try YYYY-MM first (inject day 01)
            if let Ok(dt) = NaiveDate::parse_from_str(&format!("{}-01", d), "%Y-%m-%d") {
                return dt;
            }
            // Try YYYY-MM-DD
            if let Ok(dt) = NaiveDate::parse_from_str(&d, "%Y-%m-%d") {
                return dt;
            }
            eprintln!(
                "Error: invalid date '{}'. Expected YYYY-MM or YYYY-MM-DD",
                d
            );
            std::process::exit(1);
        }
        None => Local::now().date_naive(),
    }
}

/// Return the last calendar day of the given month (handles leap years).
fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap() - Duration::days(1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap() - Duration::days(1)
    }
}

/// Compute (year, month) for `offset` months before `(year, month)`.
fn prev_month(year: i32, month: u32, offset: u32) -> (i32, u32) {
    let total = (year * 12 + month as i32) - offset as i32;
    let y = (total - 1).div_euclid(12);
    let m = (((total - 1).rem_euclid(12)) + 1) as u32;
    (y, m)
}

/// Count the number of days in the range with at least one completed pomodoro,
/// and the longest consecutive streak of active days.
fn active_day_stats(
    entries: &[Schedulable],
    first_day: NaiveDate,
    last_day: NaiveDate,
) -> (u32, u32) {
    use std::collections::HashSet;

    let mut active = HashSet::new();
    for e in entries {
        if e.kind == Kind::Pomodoro
            && e.finished_at != 0
            && let Some(utc_dt) = chrono::DateTime::from_timestamp(e.started_at, 0)
        {
            let date = utc_dt.date_naive();
            if date >= first_day && date <= last_day {
                active.insert(date);
            }
        }
    }

    let active_count = active.len() as u32;

    let mut longest = 0u32;
    let mut current = 0u32;
    let mut day = first_day;
    while day <= last_day {
        if active.contains(&day) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
        day += Duration::days(1);
    }

    (active_count, longest)
}

/// Split a month into ISO-week-aligned chunks and compute per-week aggregates.
fn compute_weekly_chunks(
    entries: &[Schedulable],
    interrupts: &[InterruptLog],
    first_day: NaiveDate,
    last_day: NaiveDate,
) -> Vec<(NaiveDate, AggregateStats)> {
    let monday = first_day - Duration::days(first_day.weekday().num_days_from_monday() as i64);
    let mut weeks = Vec::new();
    let mut current = monday;
    while current <= last_day {
        let chunk_end = (current + Duration::days(6)).min(last_day);
        let (s, e) = (
            day_bounds(current.max(first_day)).0,
            day_bounds(chunk_end).1,
        );

        let week_entries: Vec<Schedulable> = entries
            .iter()
            .filter(|en| en.started_at >= s && en.started_at <= e)
            .cloned()
            .collect();
        let week_interrupts: Vec<InterruptLog> = interrupts
            .iter()
            .filter(|l| l.created_at >= s && l.created_at <= e)
            .cloned()
            .collect();

        let agg = compute_aggregate(&week_entries, &week_interrupts);
        weeks.push((current, agg));

        current += Duration::days(7);
    }
    weeks
}

/// Generate monthly-specific hints.
fn check_monthly_hints(
    month: &AggregateStats,
    prev_months: &[((i32, u32), AggregateStats)],
    active_days: u32,
    total_days: u32,
    streak: u32,
    week_stats: &[(NaiveDate, AggregateStats)],
) -> Vec<String> {
    let mut hints = Vec::new();
    let n_prev = prev_months.len();

    // Active days ratio
    if active_days > 0 && total_days > 0 {
        let pct = active_days as f64 / total_days as f64 * 100.0;
        if pct < 70.0 {
            let day = if streak == 1 { "day" } else { "days" };
            hints.push(format!(
                "⚠ You completed at least one pomodoro on {} of {} days ({:.0}%). Aim for 70%+ for consistent momentum. Best run: {} consecutive {}.",
                active_days, total_days, pct, streak, day
            ));
        }
    }

    // Multi-month trend (need at least 2 previous months for 3-month view)
    if n_prev >= 2 {
        let three = [
            &prev_months[n_prev - 2].1,
            &prev_months[n_prev - 1].1,
            month,
        ];
        let rates: Vec<u32> = three.iter().map(|a| a.completion_rate).collect();

        if rates[0] > rates[1] && rates[1] > rates[2] && rates[2] > 0 {
            hints.push(format!(
                "📉 Your completion rate has declined for 3 months in a row ({}% → {}% → {}%). This pattern often precedes burnout — consider a recovery day or adjusting pomodoro duration.",
                rates[0], rates[1], rates[2]
            ));
        } else if rates[0] < rates[1] && rates[1] < rates[2] && rates[2] >= rates[1] {
            hints.push(format!(
                "📈 Consistent improvement over 3 months ({}% → {}% → {}%). Your adjustments are working.",
                rates[0], rates[1], rates[2]
            ));
        }
    }

    // Month-over-month changes (need at least 1 previous)
    if n_prev >= 1 {
        let prev = &prev_months[n_prev - 1].1;

        if prev.internal_count > 0 && month.internal_count < prev.internal_count {
            let drop = ((prev.internal_count - month.internal_count) as f64
                / prev.internal_count as f64
                * 100.0) as u32;
            hints.push(format!(
                "✅ Internal interruptions are down {}% vs last month. Your focus practices are paying off.",
                drop
            ));
        }
        if prev.external_count > 0 && month.external_count > prev.external_count {
            let rise = ((month.external_count - prev.external_count) as f64
                / prev.external_count as f64
                * 100.0) as u32;
            hints.push(format!(
                "⚠ External interruptions increased {}% this month. Were there specific environmental changes?",
                rise
            ));
        }

        if prev.max_focus_block > 0 && month.max_focus_block < prev.max_focus_block {
            hints.push(format!(
                "⚠ Longest focus block shrank from {} → {} consecutive pomodori. Consider whether task-switching demands have increased.",
                prev.max_focus_block, month.max_focus_block
            ));
        }
    }

    // Week-to-week variation
    let with_data: Vec<&(NaiveDate, AggregateStats)> =
        week_stats.iter().filter(|(_, a)| a.completed > 0).collect();
    if with_data.len() >= 2 {
        let max = with_data
            .iter()
            .map(|(_, a)| a.completed)
            .max()
            .unwrap_or(0);
        let min = with_data
            .iter()
            .map(|(_, a)| a.completed)
            .min()
            .unwrap_or(0);
        if min > 0 && max > min * 2 {
            hints.push(format!(
                "💡 Your most productive week had {}× the pomodori of your least productive week. What changed between weeks?",
                (max as f64 / min as f64) as u32
            ));
        }
    }

    hints
}

/// Print a monthly productivity report covering the calendar month containing
/// the given date (defaults to today), with week-by-week breakdown,
/// multi-month comparison, active-day stats, and actionable hints.
pub fn print_month_report(repo: &Repository, date: Option<String>, months_to_show: u32) {
    let date = parse_month_date(date);
    let (year, month_num) = (date.year(), date.month());

    let first_day = NaiveDate::from_ymd_opt(year, month_num, 1).unwrap();
    let last_day = last_day_of_month(year, month_num);
    let (month_start, month_end) = (day_bounds(first_day).0, day_bounds(last_day).1);

    // Fetch target month
    let (target_entries, target_interrupts) = fetch_data(repo, month_start, month_end);
    let target_agg = compute_aggregate(&target_entries, &target_interrupts);

    // Weekly breakdown
    let week_stats =
        compute_weekly_chunks(&target_entries, &target_interrupts, first_day, last_day);

    // Previous months
    let mut prev_months: Vec<((i32, u32), AggregateStats)> = Vec::new();
    for i in 1..months_to_show {
        let (py, pm) = prev_month(year, month_num, i);
        let (pf, pl) = (
            NaiveDate::from_ymd_opt(py, pm, 1).unwrap(),
            last_day_of_month(py, pm),
        );
        let (ps, pe) = (day_bounds(pf).0, day_bounds(pl).1);
        let (pe_entries, pe_interrupts) = fetch_data(repo, ps, pe);
        let pa = compute_aggregate(&pe_entries, &pe_interrupts);
        prev_months.push(((py, pm), pa));
    }

    // Active days
    let total_days = (last_day - first_day).num_days() as u32 + 1;
    let (active_count, streak) = active_day_stats(&target_entries, first_day, last_day);

    // Multi-month average completion rate
    let multi_month_avg = if months_to_show > 1 {
        let total_comp: usize =
            prev_months.iter().map(|(_, a)| a.completed).sum::<usize>() + target_agg.completed;
        let total_canc: usize =
            prev_months.iter().map(|(_, a)| a.cancelled).sum::<usize>() + target_agg.cancelled;
        let total = total_comp + total_canc;
        if total > 0 {
            (total_comp as f64 / total as f64 * 100.0) as u32
        } else {
            0
        }
    } else {
        0
    };

    // ── Print header ──────────────────────────────────────
    let mut rpt = Report::new();
    rpt.blank();
    rpt.line(format_args!("Monthly Report: {}", date.format("%B %Y")));
    if !prev_months.is_empty() {
        let labels: Vec<String> = prev_months
            .iter()
            .rev()
            .map(|((y, m), _)| {
                let d = NaiveDate::from_ymd_opt(*y, *m, 1).unwrap();
                format!("{}", d.format("%b %Y"))
            })
            .collect();
        rpt.line(format_args!("(vs {})", labels.join(" · ")));
    }
    rpt.separator(52);
    rpt.blank();

    // ── Week-by-week table ────────────────────────────────
    rpt.line("Week-by-week breakdown:");
    rpt.indent("Week        Done   Canc  Brk ▼  Brk ✗   Interr.");
    rpt.indent("─".repeat(52));
    let has_any = week_stats
        .iter()
        .any(|(_, a)| a.completed > 0 || a.cancelled > 0 || a.breaks_taken > 0);
    if !has_any {
        rpt.indent("(nothing recorded this month)");
    } else {
        let best_week = week_stats
            .iter()
            .filter(|(_, a)| a.completed > 0)
            .max_by_key(|(_, a)| a.completed);
        let worst_week = week_stats
            .iter()
            .filter(|(_, a)| a.completed > 0 || a.cancelled > 0)
            .min_by_key(|(_, a)| (a.completed as i64) - (a.cancelled as i64));

        for (week_start, agg) in &week_stats {
            if agg.completed == 0 && agg.cancelled == 0 && agg.breaks_taken == 0 {
                continue;
            }
            let star = if best_week.map(|(d, _)| *d) == Some(*week_start) {
                " ★"
            } else if worst_week.map(|(d, _)| *d) == Some(*week_start) {
                " ⊗"
            } else {
                "  "
            };
            let label = format!(
                "{} {}",
                week_start.format("%b %-d"),
                (*week_start + Duration::days(6)).format("– %-d")
            );
            rpt.line(format_args!(
                "  {:>12}{} {:>4}  {:>4}  {:>4}  {:>4}  {:>7}",
                label,
                star,
                agg.completed,
                agg.cancelled,
                agg.breaks_taken,
                agg.breaks_cancelled,
                agg.total_interruptions,
            ));
        }
    }
    rpt.blank();

    // ── Monthly summary ───────────────────────────────────
    rpt.line("Monthly summary:");
    if target_agg.completed == 0 && target_agg.cancelled == 0 && target_agg.breaks_taken == 0 {
        rpt.indent("No pomodori or breaks recorded this month.");
        rpt.blank();
        print!("{}", rpt.into_string());
        return;
    }

    let prev_rate = prev_months
        .last()
        .map(|((_, _), prev)| prev.completion_rate);
    write_metrics(
        &mut rpt,
        &target_agg,
        prev_rate,
        Some((active_count, total_days, streak)),
    );

    if multi_month_avg > 0 && months_to_show > 1 {
        rpt.indent(format_args!(
            "{}-month avg completion rate: {}%",
            months_to_show.min(12),
            multi_month_avg
        ));
    }
    rpt.blank();

    // ── Interruptions ─────────────────────────────────────
    let prev_agg = prev_months.last().map(|((_, _), a)| a);
    print_interruption_summary(&mut rpt, &target_agg, Some("prev month"), prev_agg);

    // ── Best / worst week ─────────────────────────────────
    let best_week = week_stats
        .iter()
        .filter(|(_, a)| a.completed > 0)
        .max_by_key(|(_, a)| a.completed);
    let worst_week = week_stats
        .iter()
        .filter(|(_, a)| a.completed > 0 || a.cancelled > 0)
        .min_by_key(|(_, a)| (a.completed as i64) - (a.cancelled as i64));

    if let Some((ws, agg)) = best_week
        && agg.completed > 0
    {
        rpt.line(format_args!(
            "★  Best week: {} ({} completed)",
            ws.format("%b %-d"),
            agg.completed
        ));
    }
    if let Some((ws, agg)) = worst_week
        && (agg.completed > 0 || agg.cancelled > 0)
    {
        rpt.line(format_args!(
            "⊗  Worst week: {} ({} completed, {} cancelled)",
            ws.format("%b %-d"),
            agg.completed,
            agg.cancelled
        ));
    }
    rpt.blank();

    // ── Hints ─────────────────────────────────────────────
    let hints = check_monthly_hints(
        &target_agg,
        &prev_months,
        active_count,
        total_days,
        streak,
        &week_stats,
    );
    if !hints.is_empty() {
        rpt.line("Insights:");
        for hint in &hints {
            rpt.indent(hint);
        }
        rpt.blank();
    }

    print!("{}", rpt.into_string());
}

// ── Rolling window report ─────────────────────────────────────

/// Print a rolling-window productivity report covering the last N days ending on
/// the given date (defaults to today), with day-by-day breakdown, comparison to
/// the previous window, and actionable hints.
pub fn print_last_report(repo: &Repository, date: Option<String>, days: u32) {
    let mut rpt = Report::new();

    let end_date = parse_date_or_today(date);
    let start_date = end_date - Duration::days(days as i64 - 1);

    // Current window
    let (cur_start, cur_end) = (day_bounds(start_date).0, day_bounds(end_date).1);
    let (cur_entries, cur_interrupts) = fetch_data(repo, cur_start, cur_end);

    // Previous window
    let prev_window_end = start_date - Duration::days(1);
    let prev_window_start = prev_window_end - Duration::days(days as i64 - 1);
    let (pr_start, pr_end) = (
        day_bounds(prev_window_start).0,
        day_bounds(prev_window_end).1,
    );
    let (pr_entries, pr_interrupts) = fetch_data(repo, pr_start, pr_end);

    // Day-by-day breakdown
    let day_stats = compute_day_stats(&cur_entries, &cur_interrupts, start_date, end_date);

    // Aggregates
    let cur_agg = compute_aggregate(&cur_entries, &cur_interrupts);
    let pr_agg = compute_aggregate(&pr_entries, &pr_interrupts);

    // Active days
    let total_days = days;
    let (active_count, streak) = active_day_stats(&cur_entries, start_date, end_date);

    // ── Print header ──────────────────────────────────────
    rpt.blank();
    if days == 1 {
        rpt.line(format_args!(
            "Last 1 day – {}",
            end_date.format("%b %d, %Y")
        ));
    } else {
        rpt.line(format_args!(
            "Last {} days – {} – {}",
            days,
            start_date.format("%b %d"),
            end_date.format("%b %d, %Y")
        ));
    }
    let has_prev = pr_agg.completed > 0 || pr_agg.cancelled > 0 || pr_agg.breaks_taken > 0;
    if has_prev {
        rpt.line(format_args!(
            "(prev {} days: {} – {})",
            days,
            prev_window_start.format("%b %d"),
            prev_window_end.format("%b %d")
        ));
    }
    rpt.separator(52);
    rpt.blank();

    let is_empty = cur_agg.completed == 0 && cur_agg.cancelled == 0 && cur_agg.breaks_taken == 0;
    if is_empty {
        rpt.indent("Nothing recorded in this period.");
        rpt.blank();
        print!("{}", rpt.into_string());
        return;
    }

    // ── Summary ───────────────────────────────────────────
    rpt.line("Summary:");
    let prev_rate = if has_prev {
        Some(pr_agg.completion_rate)
    } else {
        None
    };
    let active_days_opt = if active_count > 0 && total_days > 1 {
        Some((active_count, total_days, streak))
    } else {
        None
    };
    write_metrics(&mut rpt, &cur_agg, prev_rate, active_days_opt);
    rpt.blank();

    // ── Interruptions ─────────────────────────────────────
    let prev_agg = if has_prev { Some(&pr_agg) } else { None };
    print_interruption_summary(&mut rpt, &cur_agg, Some("prev"), prev_agg);

    // ── Day-by-day table ──────────────────────────────────
    if total_days > 1 {
        rpt.line("Day-by-day:");
        rpt.line("  Day       Done   Canc  Brk ▼  Brk ✗   Interr.");
        rpt.separator(50);
        let has_any = day_stats
            .iter()
            .any(|d| d.pomodori_completed > 0 || d.pomodori_cancelled > 0 || d.breaks_taken > 0);
        if !has_any {
            rpt.indent("(nothing recorded)");
        } else {
            let best_day = day_stats
                .iter()
                .filter(|d| d.pomodori_completed > 0)
                .max_by_key(|d| d.pomodori_completed);
            let worst_day = day_stats
                .iter()
                .filter(|d| d.pomodori_completed > 0 || d.pomodori_cancelled > 0)
                .min_by_key(|d| (d.pomodori_completed as i64) - (d.pomodori_cancelled as i64));

            for ds in &day_stats {
                if ds.pomodori_completed == 0 && ds.pomodori_cancelled == 0 && ds.breaks_taken == 0
                {
                    continue;
                }
                let star = if best_day.map(|d| d.date) == Some(ds.date) {
                    " ★"
                } else if worst_day.map(|d| d.date) == Some(ds.date) {
                    " ⊗"
                } else {
                    "  "
                };
                rpt.line(format_args!(
                    "  {:6}{} {:>4}  {:>4}  {:>4}  {:>4}  {:>7}",
                    ds.date.format("%b %-d"),
                    star,
                    ds.pomodori_completed,
                    ds.pomodori_cancelled,
                    ds.breaks_taken,
                    ds.breaks_cancelled,
                    ds.interruptions,
                ));
            }
        }
        rpt.blank();
    }

    // ── Hints ─────────────────────────────────────────────
    let hints = check_pattern_hints(&day_stats, &cur_agg, &pr_agg);
    if !hints.is_empty() {
        rpt.line("Insights:");
        for hint in &hints {
            rpt.indent(hint);
        }
        rpt.blank();
    }

    print!("{}", rpt.into_string());
}

/// Print a single-day report.
pub fn print_day_report(repo: &Repository, date: Option<String>) {
    use crate::{Annotation, Status, format_time};
    use std::collections::HashMap;

    let mut rpt = Report::new();

    let date = parse_date_or_today(date);
    let (start_of_day, end_of_day) = day_bounds(date);

    let entries = repo
        .entries_between(start_of_day, end_of_day)
        .unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });

    let interrupt_logs = repo
        .interrupts_between(start_of_day, end_of_day)
        .unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });

    let annotations = repo
        .annotations_between(start_of_day, end_of_day)
        .unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });

    // ── Group annotations by schedulable UUID ───────────────
    let mut ann_by_uuid: HashMap<String, Vec<&Annotation>> = HashMap::new();
    for a in &annotations {
        ann_by_uuid
            .entry(a.schedulable_uuid.to_string())
            .or_default()
            .push(a);
    }

    // ── Header ──────────────────────────────────────────────
    let day_name = date.format("%A");
    rpt.line(format_args!("Report for {} ({})", date, day_name));
    rpt.separator(35);
    rpt.blank();

    if entries.is_empty() {
        rpt.line("Nothing recorded for this day.");
        print!("{}", rpt.into_string());
        return;
    }

    // ── Entry list with annotations ─────────────────────────
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

        rpt.line(format_args!(
            " {:>5} - {:<5}  {:<9} ({:>2} min)  {}{}",
            start,
            end,
            format!("{}", entry.kind),
            entry.duration,
            status_icon,
            interrupt_info,
        ));

        // Annotations for this entry
        if let Some(notes) = ann_by_uuid.get(&entry.uuid.to_string()) {
            for note in notes {
                rpt.line(format_args!("    \u{2192} {}", note.body));
            }
        }
    }
    rpt.blank();

    // ── Metrics ─────────────────────────────────────────────
    let agg = compute_aggregate(&entries, &interrupt_logs);

    rpt.line(format_args!(
        "Pomodori    {} completed  \u{00b7}  {} cancelled  \u{00b7}  {}% completion rate",
        agg.completed, agg.cancelled, agg.completion_rate
    ));
    rpt.line(format_args!(
        "Breaks      {} taken      \u{00b7}  {} cancelled",
        agg.breaks_taken, agg.breaks_cancelled
    ));
    if agg.completed > 0 && agg.breaks_taken > 0 {
        rpt.line(format_args!(
            "Ratio       {:.1} break per pomodoro  {}",
            agg.break_ratio,
            ratio_indicator(agg.break_ratio)
        ));
    }
    rpt.blank();

    if agg.max_focus_block > 1 {
        rpt.line(format_args!(
            "Longest focus block:  {} consecutive pomodori without interruption",
            agg.max_focus_block
        ));
        rpt.blank();
    }

    // ── Interruptions ─────────────────────────────────────
    print_interruption_summary(&mut rpt, &agg, None, None);

    print!("{}", rpt.into_string());
}
