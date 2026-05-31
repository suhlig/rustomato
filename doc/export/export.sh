#!/usr/bin/env bash
set -euo pipefail

# Weekly pomodoro report — full pipeline
# Usage:  cd doc/export && bash export.sh
# Requires: qsv, typst, gnuplot, jq

# 1. Export from rustomato (or use example.csv for testing)
rustomato export --from 2026-04-20 --to 2026-04-26 > export.csv

# 2. Aggregate daily pomodoro counts with QSV
qsv search "pomodoro" export.csv |
  qsv search "finished" |
  qsv select started_at |
  qsv behead |
  sed 's/T.*//' |
  sort |
  uniq -c |
  awk '{print $2","$1}' > daily_counts.csv

# 3. Extract annotations
cat export.csv |
  mlr --c2j cat |
  jq -n 'inputs |
    select(.annotations != "") |
    .annotations | fromjson[] |
    [.created_at[0:10], .body] |
    @csv' > annotations.csv

# 4. Render report (includes annotations table)
typst compile report.typ report.pdf

# 5. Generate chart
gnuplot chart.gnuplot

echo "Created report.pdf and daily_chart.pdf"
