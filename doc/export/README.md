# Exporting data for external reports

The `rustomato export` command writes pomodori and breaks as a **CSV file** to stdout, so you can build custom reports without accessing the SQLite database directly.

```
rustomato export [--from YYYY-MM-DD] [--to YYYY-MM-DD] > data.csv
```

Each row is one schedulable (pomodoro or break). Timestamps are ISO 8601 with timezone offset so spreadsheets parse them natively. Annotations are embedded as a JSON string column — no information is lost, but the data stays flat and pivotable.

| Column | Description |
|---|---|
| `uuid` | Unique identifier |
| `kind` | `pomodoro` or `break` |
| `planned_duration` | Planned length in minutes |
| `started_at` | ISO 8601 start timestamp |
| `finished_at` | ISO 8601 finish timestamp (empty if not finished) |
| `cancelled_at` | ISO 8601 cancel timestamp (empty if not cancelled) |
| `status` | Derived state: `finished`, `cancelled`, `stale`, or `active` |
| `interruptions` | Number of interruptions recorded |
| `elapsed_min` | Actual duration in minutes (from timestamps, not the timer) |
| `annotations` | JSON array of `{uuid, body, created_at}` objects, or empty |

Because the output is plain CSV, you can pipe it into any data tool — QSV, Miller, pandas, R, or a spreadsheet.

There is also a more [detailed example](export/README.md).
