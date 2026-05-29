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
    let date = parse_date_or_today(date);
    let weekday = date.weekday().num_days_from_monday(); // Mon=0 … Sun=6
    let monday = date - Duration::days(weekday as i64);
    let sunday = monday + Duration::days(6);

    // Previous week range
    let prev_monday = monday - Duration::days(7);
    let prev_sunday = sunday - Duration::days(7);

    // Fetch data for both weeks
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

    // ── Print header ──────────────────────────────────────
    println!();
    println!(
        "Weekly Report: {} – {}",
        monday.format("%b %d"),
        sunday.format("%b %d, %Y")
    );
    if prev_week.completed > 0 || prev_week.cancelled > 0 || prev_week.breaks_taken > 0 {
        println!(
            "(vs week of {} – {})",
            prev_monday.format("%b %d"),
            prev_sunday.format("%b %d")
        );
    }
    println!("{}", "─".repeat(52));
    println!();

    // ── Day-by-day table ──────────────────────────────────
    println!("Day-by-day breakdown:");
    println!("  Day       Done   Canc  Brk ▼  Brk ✗   Interr.");
    println!("  {}", "─".repeat(50));
    let has_any = day_stats
        .iter()
        .any(|d| d.pomodori_completed > 0 || d.pomodori_cancelled > 0 || d.breaks_taken > 0);
    if !has_any {
        println!("  (nothing recorded this week)");
    } else {
        for ds in &day_stats {
            let day_name = ds.date.format("%a");
            let star = if Some(ds.date) == best_day.map(|d| d.date) {
                " ★"
            } else if Some(ds.date) == worst_day.map(|d| d.date) {
                " ⊗"
            } else {
                "  "
            };
            println!(
                "  {:6}{} {:>4}  {:>4}  {:>4}  {:>4}  {:>7}",
                day_name,
                star,
                ds.pomodori_completed,
                ds.pomodori_cancelled,
                ds.breaks_taken,
                ds.breaks_cancelled,
                ds.interruptions,
            );
        }
    }
    println!();

    // ── Weekly summary ────────────────────────────────────
    println!("Weekly summary:");
    if week.completed == 0 && week.cancelled == 0 && week.breaks_taken == 0 {
        println!("  No pomodori or breaks recorded this week.");
        println!();
        return;
    }

    let prev_rate_str = if prev_week.completed > 0 || prev_week.cancelled > 0 {
        format!(" (prev: {}%)", prev_week.completion_rate)
    } else {
        String::new()
    };
    println!(
        "  Pomodori:     {} completed · {} cancelled  ·  {}% completion rate{}",
        week.completed, week.cancelled, week.completion_rate, prev_rate_str
    );
    println!(
        "  Breaks:       {} taken · {} cancelled",
        week.breaks_taken, week.breaks_cancelled
    );

    if week.completed > 0 && week.breaks_taken > 0 {
        let ratio_indicator = if (0.5..=2.0).contains(&week.break_ratio) {
            "✓"
        } else {
            "⚠"
        };
        println!(
            "  Ratio:        {:.1} break per pomodoro  {}",
            week.break_ratio, ratio_indicator
        );
    }

    if week.max_focus_block > 0 {
        println!(
            "  Focus block:  {} consecutive pomodori without interruption",
            week.max_focus_block
        );
    }
    println!();

    // ── Interruptions ─────────────────────────────────────
    println!("Interruptions:");
    println!(
        "  Total:        {} ({:.1} avg per pomodoro)",
        week.total_interruptions, week.avg_interruptions
    );
    let total_logged = week.internal_count + week.external_count;
    if total_logged > 0 {
        let internal_pct = (week.internal_count as f64 / total_logged as f64 * 100.0) as u32;
        let external_pct = (week.external_count as f64 / total_logged as f64 * 100.0) as u32;
        println!(
            "  Internal:     {} ({}%)",
            week.internal_count, internal_pct
        );
        println!(
            "  External:     {} ({}%)",
            week.external_count, external_pct
        );

        let prev_total = prev_week.internal_count + prev_week.external_count;
        if prev_total > 0 {
            let prev_internal_pct =
                (prev_week.internal_count as f64 / prev_total as f64 * 100.0) as u32;
            println!(
                "  (prev week: {} internal · {} external, {}% internal)",
                prev_week.internal_count, prev_week.external_count, prev_internal_pct
            );
        }
    } else if week.total_interruptions > 0 {
        println!("  (Kind breakdown not available for interruptions recorded before the upgrade)");
    }
    println!();

    // ── Best / worst day ──────────────────────────────────
    if let Some(best) = best_day {
        println!(
            "★  Best day: {} ({} completed)",
            best.date.format("%A"),
            best.pomodori_completed
        );
    }
    if let Some(worst) = worst_day {
        let has_any = worst.pomodori_completed > 0 || worst.pomodori_cancelled > 0;
        if has_any {
            println!(
                "⊗  Worst day: {} ({} completed, {} cancelled)",
                worst.date.format("%A"),
                worst.pomodori_completed,
                worst.pomodori_cancelled
            );
        }
    }
    println!();

    // ── Hints ─────────────────────────────────────────────
    let hints = check_pattern_hints(&day_stats, &week, &prev_week);
    if !hints.is_empty() {
        println!("Insights:");
        for hint in &hints {
            println!("  {}", hint);
        }
        println!();
    }
}

// ── Interruption patterns report ────────────────────────────

/// Print an interruption pattern report covering the last N days from the given
/// date, broken down by hour of day and day of week with internal/external split.
pub fn print_interruptions_report(repo: &Repository, date: Option<String>, days: u32) {
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

    println!();
    println!("Interruption Patterns: {}", period_label);
    println!("{}", "─".repeat(52));
    println!();

    if interrupts.is_empty() {
        let total_interruptions: i64 = entries
            .iter()
            .filter(|e| e.kind == Kind::Pomodoro)
            .map(|e| e.interruptions)
            .sum();
        if total_interruptions > 0 {
            println!(
                "  {} interruption(s) recorded via counter in this period, but the interrupt log\n  (which provides kind/hour/day breakdown) is empty. Interruptions recorded\n  before the upgrade are not included in this report.",
                total_interruptions
            );
        } else {
            println!("  No interruptions recorded in this period.");
        }
        println!();
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
    println!("By hour of day:");
    println!("  Hour      Total  Internal  External");
    println!("  {}", "─".repeat(40));
    for hour in 0..24 {
        let (total, internal, external) = by_hour.get(&hour).copied().unwrap_or((0, 0, 0));
        if total > 0 {
            let marker = if total == max_hour_total && total > 0 {
                "  ⚠"
            } else {
                "   "
            };
            println!(
                "  {:02}:00    {:>5}  {:>8}  {:>8}{}",
                hour, total, internal, external, marker
            );
        }
    }
    println!();

    // ── Day-of-week breakdown ─────────────────────────────
    let weekday_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    println!("By day of week:");
    println!("  Day       Total  Internal  External");
    println!("  {}", "─".repeat(40));
    for wd in 0..7 {
        let (total, internal, external) = by_weekday.get(&wd).copied().unwrap_or((0, 0, 0));
        if total > 0 {
            let marker = if total == max_wd_total && total > 0 {
                "  ⚠"
            } else {
                "   "
            };
            println!(
                "  {:6}  {:>5}  {:>8}  {:>8}{}",
                weekday_names[wd as usize], total, internal, external, marker
            );
        }
    }
    println!();

    // ── Summary stats ─────────────────────────────────────
    let total_count: usize = interrupts.len();
    let internal_count = interrupts
        .iter()
        .filter(|l| l.kind == InterruptionKind::Internal)
        .count();
    let external_count = total_count - internal_count;
    let internal_pct = (internal_count as f64 / total_count as f64 * 100.0) as u32;
    let external_pct = (external_count as f64 / total_count as f64 * 100.0) as u32;

    println!(
        "Total: {} interruptions ({} internal · {} external, {}% / {}%)",
        total_count, internal_count, external_count, internal_pct, external_pct
    );
    println!();
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
    println!();
    println!("Monthly Report: {}", date.format("%B %Y"));
    if !prev_months.is_empty() {
        let labels: Vec<String> = prev_months
            .iter()
            .rev()
            .map(|((y, m), _)| {
                let d = NaiveDate::from_ymd_opt(*y, *m, 1).unwrap();
                format!("{}", d.format("%b %Y"))
            })
            .collect();
        println!("(vs {})", labels.join(" · "));
    }
    println!("{}", "─".repeat(52));
    println!();

    // ── Week-by-week table ────────────────────────────────
    println!("Week-by-week breakdown:");
    println!("  Week        Done   Canc  Brk ▼  Brk ✗   Interr.");
    println!("  {}", "─".repeat(52));
    let has_any = week_stats
        .iter()
        .any(|(_, a)| a.completed > 0 || a.cancelled > 0 || a.breaks_taken > 0);
    if !has_any {
        println!("  (nothing recorded this month)");
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
            println!(
                "  {:>12}{} {:>4}  {:>4}  {:>4}  {:>4}  {:>7}",
                label,
                star,
                agg.completed,
                agg.cancelled,
                agg.breaks_taken,
                agg.breaks_cancelled,
                agg.total_interruptions,
            );
        }
    }
    println!();

    // ── Monthly summary ───────────────────────────────────
    println!("Monthly summary:");
    if target_agg.completed == 0 && target_agg.cancelled == 0 && target_agg.breaks_taken == 0 {
        println!("  No pomodori or breaks recorded this month.");
        println!();
        return;
    }

    let prev_rate_str = if let Some(((_, _), prev)) = prev_months.last() {
        if prev.completed > 0 || prev.cancelled > 0 {
            format!(" (prev: {}%", prev.completion_rate)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let avg_str = if multi_month_avg > 0 && months_to_show > 1 {
        format!(
            "{}  |  {}-month avg: {}%",
            prev_rate_str,
            months_to_show.min(12),
            multi_month_avg
        )
    } else {
        prev_rate_str
    };

    println!(
        "  Pomodori:     {} completed · {} cancelled  ·  {}% completion rate{}",
        target_agg.completed, target_agg.cancelled, target_agg.completion_rate, avg_str
    );
    println!(
        "  Breaks:       {} taken · {} cancelled",
        target_agg.breaks_taken, target_agg.breaks_cancelled
    );

    if target_agg.completed > 0 && target_agg.breaks_taken > 0 {
        let ratio_indicator = if (0.5..=2.0).contains(&target_agg.break_ratio) {
            "✓"
        } else {
            "⚠"
        };
        println!(
            "  Ratio:        {:.1} break per pomodoro  {}",
            target_agg.break_ratio, ratio_indicator
        );
    }

    if target_agg.max_focus_block > 0 {
        println!(
            "  Focus block:  {} consecutive pomodori without interruption",
            target_agg.max_focus_block
        );
    }

    if active_count > 0 {
        let day = if streak == 1 { "day" } else { "days" };
        println!(
            "  Active days:  {} of {} ({:.0}%)  ·  Best streak: {} {}",
            active_count,
            total_days,
            active_count as f64 / total_days as f64 * 100.0,
            streak,
            day
        );
    }
    println!();

    // ── Interruptions ─────────────────────────────────────
    println!("Interruptions:");
    println!(
        "  Total:        {} ({:.1} avg per pomodoro)",
        target_agg.total_interruptions, target_agg.avg_interruptions
    );
    let total_logged = target_agg.internal_count + target_agg.external_count;
    if total_logged > 0 {
        let internal_pct = (target_agg.internal_count as f64 / total_logged as f64 * 100.0) as u32;
        let external_pct = (target_agg.external_count as f64 / total_logged as f64 * 100.0) as u32;
        println!(
            "  Internal:     {} ({}%)",
            target_agg.internal_count, internal_pct
        );
        println!(
            "  External:     {} ({}%)",
            target_agg.external_count, external_pct
        );

        if let Some(((_, _), prev)) = prev_months.last() {
            let prev_total = prev.internal_count + prev.external_count;
            if prev_total > 0 {
                let prev_internal_pct =
                    (prev.internal_count as f64 / prev_total as f64 * 100.0) as u32;
                println!(
                    "  (prev month: {} internal · {} external, {}% internal)",
                    prev.internal_count, prev.external_count, prev_internal_pct
                );
            }
        }
    } else if target_agg.total_interruptions > 0 {
        println!("  (Kind breakdown not available for interruptions recorded before the upgrade)");
    }
    println!();

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
        println!(
            "★  Best week: {} ({} completed)",
            ws.format("%b %-d"),
            agg.completed
        );
    }
    if let Some((ws, agg)) = worst_week
        && (agg.completed > 0 || agg.cancelled > 0)
    {
        println!(
            "⊗  Worst week: {} ({} completed, {} cancelled)",
            ws.format("%b %-d"),
            agg.completed,
            agg.cancelled
        );
    }
    println!();

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
        println!("Insights:");
        for hint in &hints {
            println!("  {}", hint);
        }
        println!();
    }
}
