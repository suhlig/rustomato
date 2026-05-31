use crate::persistence::Repository;
use crate::{Annotation, Schedulable};
use chrono::{Local, NaiveDate, TimeZone};
use std::fmt::Write;

/// Export entries as CSV to stdout.
pub fn cmd_export(repo: &Repository, from: Option<&str>, to: Option<&str>) {
    let start_ts = match from {
        Some(date_str) => {
            let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap_or_else(|e| {
                eprintln!(
                    "Error: invalid --from date '{}': {}. Expected format: YYYY-MM-DD",
                    date_str, e
                );
                std::process::exit(1);
            });
            day_start_ts(date)
        }
        None => 0,
    };

    let end_ts = match to {
        Some(date_str) => {
            let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap_or_else(|e| {
                eprintln!(
                    "Error: invalid --to date '{}': {}. Expected format: YYYY-MM-DD",
                    date_str, e
                );
                std::process::exit(1);
            });
            day_end_ts(date)
        }
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    };

    let entries = repo.entries_between(start_ts, end_ts).unwrap_or_else(|e| {
        eprintln!("Error: {}.", e);
        std::process::exit(1);
    });

    // CSV header
    println!(
        "uuid,kind,planned_duration,started_at,finished_at,cancelled_at,\
         status,interruptions,elapsed_min,annotations"
    );

    for entry in &entries {
        let annotations = repo.annotations_for(entry.uuid).unwrap_or_default();
        println!("{}", format_row(entry, &annotations));
    }
}

// ── Helpers ────────────────────────────────────────────────────

fn day_start_ts(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .and_then(|dt| dt.and_local_timezone(Local).earliest())
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

fn day_end_ts(date: NaiveDate) -> i64 {
    date.and_hms_opt(23, 59, 59)
        .and_then(|dt| dt.and_local_timezone(Local).earliest())
        .map(|dt| dt.timestamp())
        .unwrap_or(i64::MAX)
}

/// Format a Unix timestamp as ISO 8601 with timezone offset, or empty string for 0.
fn format_ts(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%+").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn status_str(s: &Schedulable) -> &'static str {
    s.status().as_str()
}

/// Compute elapsed time in minutes (based on finished_at or cancelled_at).
/// Returns empty if the entry is still active or new.
fn elapsed_min(s: &Schedulable) -> String {
    let end = if s.finished_at != 0 {
        s.finished_at
    } else if s.cancelled_at != 0 {
        s.cancelled_at
    } else {
        return String::new();
    };
    let secs = end - s.started_at;
    if secs >= 0 {
        (secs / 60).to_string()
    } else {
        String::new()
    }
}

/// Build a JSON array of annotation objects. Empty string when there are no annotations.
///
/// Example: `[{"uuid":"abc123","body":"feeling focused","created_at":"2026-05-31T09:45:00+02:00"}]`
fn format_annotations_json(annotations: &[Annotation]) -> String {
    if annotations.is_empty() {
        return String::new();
    }

    let mut json = String::from('[');
    for (i, ann) in annotations.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push('{');
        write!(
            json,
            r#""uuid":"{}","body":"{}","created_at":"{}""#,
            ann.uuid,
            escape_json_string(&ann.body),
            format_ts(ann.created_at),
        )
        .unwrap();
        json.push('}');
    }
    json.push(']');
    json
}

/// Escape a string for JSON. Handles quotes, backslashes, newlines, and other control chars.
fn escape_json_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => escaped.push_str(r#"\""#),
            '\\' => escaped.push_str(r#"\\"#),
            '\n' => escaped.push_str(r#"\n"#),
            '\r' => escaped.push_str(r#"\r"#),
            '\t' => escaped.push_str(r#"\t"#),
            c if c.is_control() => {
                let _ = write!(escaped, "\\u{:04x}", c as u32);
            }
            c => escaped.push(c),
        }
    }
    escaped
}

/// Quote a CSV field if it contains commas, double quotes, or newlines.
fn csv_quote(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        let mut escaped = String::with_capacity(field.len() + 2);
        escaped.push('"');
        for ch in field.chars() {
            if ch == '"' {
                escaped.push_str("\"\"");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('"');
        escaped
    } else {
        field.to_string()
    }
}

/// Format one CSV row from a schedulable and its annotations.
fn format_row(s: &Schedulable, annotations: &[Annotation]) -> String {
    let ann_json = format_annotations_json(annotations);
    format!(
        "{},{},{},{},{},{},{},{},{},{}",
        s.uuid,
        s.kind,
        s.duration,
        format_ts(s.started_at),
        format_ts(s.finished_at),
        format_ts(s.cancelled_at),
        status_str(s),
        s.interruptions,
        elapsed_min(s),
        csv_quote(&ann_json),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_zero_is_empty() {
        assert_eq!(format_ts(0), "");
    }

    #[test]
    fn test_elapsed_min_finished() {
        let mut s = Schedulable::new(1, crate::Kind::Pomodoro, 25);
        s.started_at = 1000;
        s.finished_at = 2500;
        assert_eq!(elapsed_min(&s), "25");
    }

    #[test]
    fn test_elapsed_min_cancelled() {
        let mut s = Schedulable::new(1, crate::Kind::Pomodoro, 25);
        s.started_at = 1000;
        s.cancelled_at = 1900;
        assert_eq!(elapsed_min(&s), "15");
    }

    #[test]
    fn test_elapsed_min_active_is_empty() {
        let s = Schedulable::new(1, crate::Kind::Pomodoro, 25);
        assert_eq!(elapsed_min(&s), "");
    }

    #[test]
    fn test_status_str_new() {
        let s = Schedulable::new(1, crate::Kind::Pomodoro, 25);
        assert_eq!(status_str(&s), "new");
    }

    #[test]
    fn test_escape_json_simple() {
        assert_eq!(escape_json_string("hello"), "hello");
    }

    #[test]
    fn test_escape_json_quotes() {
        assert_eq!(escape_json_string("say \"hi\""), r#"say \"hi\""#);
    }

    #[test]
    fn test_escape_json_newline() {
        assert_eq!(escape_json_string("a\nb"), r"a\nb");
    }

    #[test]
    fn test_csv_quote_needed() {
        assert_eq!(csv_quote("a,b"), r#""a,b""#);
        assert_eq!(csv_quote("say \"hi\""), r#""say ""hi""""#);
    }

    #[test]
    fn test_csv_quote_not_needed() {
        assert_eq!(csv_quote("hello"), "hello");
        assert_eq!(csv_quote("42"), "42");
    }

    #[test]
    fn test_format_annotations_json_empty() {
        assert_eq!(format_annotations_json(&[]), "");
    }

    #[test]
    fn test_format_annotations_json_single() {
        let ann = Annotation {
            uuid: crate::SqlUuid::default(),
            schedulable_uuid: crate::SqlUuid::default(),
            body: "test".to_string(),
            created_at: 1000,
        };
        let json = format_annotations_json(&[ann]);
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
        assert!(json.contains(r#""body":"test""#));
    }

    #[test]
    fn test_format_annotations_json_escapes_body() {
        let ann = Annotation {
            uuid: crate::SqlUuid::default(),
            schedulable_uuid: crate::SqlUuid::default(),
            body: "say \"hi\"".to_string(),
            created_at: 1000,
        };
        let json = format_annotations_json(&[ann]);
        assert!(json.contains(r#"say \"hi\""#));
    }

    #[test]
    fn test_format_row_basic() {
        let mut s = Schedulable::new(42, crate::Kind::Pomodoro, 25);
        s.started_at = 1000;
        s.finished_at = 2500;
        let row = format_row(&s, &[]);
        assert!(row.contains("pomodoro"));
        assert!(row.contains("25")); // planned_duration
        assert!(row.contains("finished"));
    }

    #[test]
    fn test_format_row_with_annotations() {
        let mut s = Schedulable::new(1, crate::Kind::Pomodoro, 25);
        s.started_at = 1000;
        s.finished_at = 2500;
        let ann = Annotation {
            uuid: crate::SqlUuid::default(),
            schedulable_uuid: s.uuid,
            body: "note".to_string(),
            created_at: 1500,
        };
        let row = format_row(&s, &[ann]);
        assert!(row.contains("note"));
        // JSON keys are CSV-escaped (""body""), so check for the body value only
        assert!(!row.contains("\"body\":\"note\"")); // would be raw JSON, not CSV
    }
}
