#!/usr/bin/env bash
set -euo pipefail

# Weekly pomodoro report — full pipeline
# Usage:  cd doc/export && bash export.sh
# Requires: miller, typst, gnuplot, jq

# 1. Export from rustomato (or use example.csv for testing)
rustomato export --from 2026-04-20 --to 2026-04-26 >export.csv

# 2. Aggregate daily pomodoro counts
# mlr handles CSV quoting correctly (qsv chokes on the annotations JSON column)
mlr --csv filter '$kind == "pomodoro" && $status == "finished"' \
    then put '$date = substr($started_at, 0, 9)' \
    then stats1 -a count -f date -g date \
    then rename date_count,pomodori \
    export.csv >daily_counts.csv

# 3. Extract annotations
mlr --c2j cat export.csv |
    jq '.[] | select(.annotations != "") | .annotations | fromjson[] | [.created_at[0:10], .body] | @csv' -r >annotations.csv

# 4. Generate chart (must be before typst — SVG is embedded in report)
gnuplot chart.gnuplot

# 5. Render report (includes annotations table and chart)
typst compile report.typ report.pdf

# 6. Open report
echo "Report saved to report.pdf"
