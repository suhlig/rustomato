This document walks through a concrete export example: turning a week of pomodoro data into a PDF report using **Miller** (data prep), **Typst** (layout), and **Gnuplot** (charts). All four are single binaries — no language runtime required.

## Example data

Assume `rustomato export` produced the following CSV for the week of April 20–26, 2026:

```csv
uuid,kind,planned_duration,started_at,finished_at,cancelled_at,status,interruptions,elapsed_min,annotations
a1b2c3d4e5f6g7h8,pomodoro,25,2026-04-20T09:15:00+02:00,2026-04-20T09:40:00+02:00,,finished,0,25,
b2c3d4e5f6g7h8i9,break,5,2026-04-20T09:45:00+02:00,2026-04-20T09:50:00+02:00,,finished,0,5,
c3d4e5f6g7h8i9j0,pomodoro,25,2026-04-20T09:55:00+02:00,2026-04-20T10:20:00+02:00,,finished,2,25,"[{""body"":""slack notification"",""created_at"":""2026-04-20T10:05:00+02:00""}]"
d4e5f6g7h8i9j0k1,break,5,2026-04-20T10:25:00+02:00,2026-04-20T10:30:00+02:00,,finished,0,5,
e5f6g7h8i9j0k1l2,pomodoro,25,2026-04-20T10:35:00+02:00,2026-04-20T11:00:00+02:00,,finished,1,25,
f6g7h8i9j0k1l2m3,break,15,2026-04-20T11:05:00+02:00,2026-04-20T11:20:00+02:00,,finished,0,15,
g7h8i9j0k1l2m3n4,pomodoro,25,2026-04-20T11:25:00+02:00,2026-04-20T11:50:00+02:00,,finished,0,25,
h8i9j0k1l2m3n4o5,break,5,2026-04-20T11:55:00+02:00,2026-04-20T12:00:00+02:00,,finished,0,5,
i9j0k1l2m3n4o5p6,pomodoro,25,2026-04-21T09:00:00+02:00,,2026-04-21T09:15:00+02:00,cancelled,3,15,"[{""body"":""urgent client call"",""created_at"":""2026-04-21T09:05:00+02:00""},{""body"":":"",pager alert"",""created_at"":""2026-04-21T09:10:00+02:00""},{""body"":""team standup"",""created_at"":""2026-04-21T09:14:00+02:00""}]"
j0k1l2m3n4o5p6q7,pomodoro,25,2026-04-21T10:00:00+02:00,2026-04-21T10:25:00+02:00,,finished,1,25,
k1l2m3n4o5p6q7r8,break,5,2026-04-21T10:30:00+02:00,2026-04-21T10:35:00+02:00,,finished,0,5,
l2m3n4o5p6q7r8s9,pomodoro,25,2026-04-21T10:40:00+02:00,2026-04-21T11:05:00+02:00,,finished,0,25,"[{""body"":""deep focus session"",""created_at"":""2026-04-21T10:50:00+02:00""}]"
m3n4o5p6q7r8s9t0,break,5,2026-04-21T11:10:00+02:00,2026-04-21T11:15:00+02:00,,finished,0,5,
n4o5p6q7r8s9t0u1,pomodoro,25,2026-04-22T08:30:00+02:00,2026-04-22T08:55:00+02:00,,finished,1,25,
o5p6q7r8s9t0u1v2,break,5,2026-04-22T09:00:00+02:00,2026-04-22T09:05:00+02:00,,finished,0,5,
p6q7r8s9t0u1v2w3,pomodoro,25,2026-04-22T09:10:00+02:00,2026-04-22T09:35:00+02:00,,finished,1,25,"[{""body"":""code review ping"",""created_at"":""2026-04-22T09:20:00+02:00""}]"
q7r8s9t0u1v2w3x4,break,5,2026-04-22T09:40:00+02:00,2026-04-22T09:45:00+02:00,,finished,0,5,
r8s9t0u1v2w3x4y5,pomodoro,25,2026-04-22T09:50:00+02:00,2026-04-22T10:15:00+02:00,,finished,2,25,
s9t0u1v2w3x4y5z6,break,5,2026-04-22T10:20:00+02:00,2026-04-22T10:25:00+02:00,,finished,0,5,
t0u1v2w3x4y5z6a7,pomodoro,25,2026-04-22T10:30:00+02:00,2026-04-22T10:55:00+02:00,,finished,1,25,
u1v2w3x4y5z6a7b8,break,5,2026-04-22T11:00:00+02:00,2026-04-22T11:05:00+02:00,,finished,0,5,
v2w3x4y5z6a7b8c9,pomodoro,25,2026-04-22T14:00:00+02:00,2026-04-22T14:25:00+02:00,,finished,0,25,
w3x4y5z6a7b8c9d0,pomodoro,25,2026-04-23T10:00:00+02:00,2026-04-23T10:25:00+02:00,,finished,0,25,"[{""body"":""code review"",""created_at"":""2026-04-23T10:15:00+02:00""}]"
x4y5z6a7b8c9d0e1,break,5,2026-04-23T10:30:00+02:00,2026-04-23T10:35:00+02:00,,finished,0,5,
y5z6a7b8c9d0e1f2,pomodoro,25,2026-04-23T10:40:00+02:00,2026-04-23T11:05:00+02:00,,finished,1,25,
z6a7b8c9d0e1f2g3,break,15,2026-04-23T11:10:00+02:00,2026-04-23T11:25:00+02:00,,finished,0,15,
a7b8c9d0e1f2g3h4,pomodoro,25,2026-04-24T09:00:00+02:00,2026-04-24T09:25:00+02:00,,finished,0,25,
b8c9d0e1f2g3h4i5,break,5,2026-04-24T09:30:00+02:00,2026-04-24T09:35:00+02:00,,finished,0,5,
c9d0e1f2g3h4i5j6,pomodoro,25,2026-04-24T09:40:00+02:00,2026-04-24T10:05:00+02:00,,finished,0,25,"[{""body"":""writing specs"",""created_at"":""2026-04-24T09:50:00+02:00""}]"
d0e1f2g3h4i5j6k7,break,5,2026-04-24T10:10:00+02:00,2026-04-24T10:15:00+02:00,,finished,0,5,
e1f2g3h4i5j6k7l8,pomodoro,25,2026-04-24T10:20:00+02:00,2026-04-24T10:45:00+02:00,,finished,0,25,"[{""body"":""implementation done"",""created_at"":""2026-04-24T10:40:00+02:00""}]"
f2g3h4i5j6k7l8m9,break,5,2026-04-24T10:50:00+02:00,2026-04-24T10:55:00+02:00,,finished,0,5,
g3h4i5j6k7l8m9n0,pomodoro,25,2026-04-24T11:00:00+02:00,2026-04-24T11:25:00+02:00,,finished,1,25,
```

That's 33 entries across the working week: 18 pomodori (17 finished, 1 cancelled), 13 short breaks, 2 long breaks, and a weekend with nothing. Several pomodori carry annotations.

## Tool installation

On macOS with Homebrew:

```sh
brew install miller typst gnuplot jq
```

## Step 1 — Prepare the data with Miller (mlr)

The raw CSV contains every schedulable. For a weekly report we want:

- **Daily pomodoro count** (finished only — cancelled don't count)
- **Daily interruption count**
- **Weekly summary stats**

We use [Miller](https://miller.readthedocs.io/) for data preparation because it handles complex CSV quoting correctly (the annotations JSON column contains commas and quotes that confuse simpler CSV parsers).

```sh
# Filter finished pomodori, extract the date, count per day
mlr --csv filter '$kind == "pomodoro" && $status == "finished"' \
  then put '$date = strftime($started_at, "%Y-%m-%d")' \
  then stats1 -a count -f date -g date \
  then rename date_count,pomodori \
  example.csv > daily_counts.csv
```

The result (`daily_counts.csv`):

```csv
date,pomodori
2026-04-20,4
2026-04-21,2
2026-04-22,5
2026-04-23,2
2026-04-24,4
```

## Step 2 — Compute summary stats

A quick shell pipeline to get aggregate numbers for the report header:

```sh
# Total finished pomodori (exclude header)
TOTAL=$(tail -n +2 daily_counts.csv | awk -F, '{s+=$2}END{print s}')

# Total interruptions for finished pomodori
INTERRUPTIONS=$(mlr --csv filter '$kind == "pomodoro" && $status == "finished"' \
  then stats1 -a sum -f interruptions \
  example.csv | tail -n +2)

# Days with at least one pomodoro
DAYS_WORKED=$(( $(wc -l < daily_counts.csv | tr -d ' ') - 1 ))

echo "Total pomodori: $TOTAL"
echo "Total interruptions: $INTERRUPTIONS"
echo "Days worked: $DAYS_WORKED"
```

Output:

```
Total pomodori: 17
Total interruptions: 11
Days worked: 5
```

## Step 3 — Render the PDF with Typst

First, extract the annotations from the JSON column into a separate CSV file:

```sh
cat example.csv |
  mlr --c2j cat |
  jq -n 'inputs |
    select(.annotations != "") |
    .annotations | fromjson[] |
    [.created_at[0:10], .body] |
    @csv' > annotations.csv
```

This produces `annotations.csv`:

```csv
"2026-04-20","slack notification"
"2026-04-21","urgent client call"
"2026-04-21","pager alert"
"2026-04-21","team standup"
"2026-04-21","deep focus session"
"2026-04-22","code review ping"
"2026-04-23","code review"
"2026-04-24","writing specs"
"2026-04-24","implementation done"
```

<details>
<summary>Alternative using Python (no extra install needed)</summary>

```bash
python3 -c "
import csv, json, sys
for row in csv.DictReader(sys.stdin):
    if row['annotations']:
        for ann in json.loads(row['annotations']):
            date = ann['created_at'][:10]
            print(f'{date},{ann[\"body\"]}')
" < example.csv > annotations.csv
```

</details>

Now create `report.typ` with the full report layout — Summary, Daily Breakdown, and Annotations table:

```typst
// report.typ — Weekly pomodoro report
// Usage:  typst compile report.typ report.pdf
// Expects: daily_counts.csv, annotations.csv, daily_chart.pdf (optional)

#let data = csv("daily_counts.csv")

#set page(margin: 2.5cm)
#set text(font: "Helvetica", size: 11pt)

= Weekly Pomodoro Report
#smallcaps[April 20–26, 2026]

#line(length: 100%)

#v(1em)

== Summary

#let total = data.slice(1).map(r => int(r.at(1))).sum()
#let days = data.len() - 1
#let avg = if days > 0 { calc.round(total / days, 1) } else { 0 }
#let best = data.slice(1).map(r => int(r.at(1))).max()

#grid(
  columns: (1fr, 1fr, 1fr),
  align(center)[
    #text(size: 24pt, weight: "bold", str(total)) \
    Pomodori
  ],
  align(center)[
    #text(size: 24pt, weight: "bold", str(avg)) \
    Avg / day
  ],
  align(center)[
    #text(size: 24pt, weight: "bold", str(best)) \
    Best day
  ],
)

#v(2em)

== Daily Breakdown

#table(
  columns: (auto, auto, auto, auto),
  stroke: none,
  [*Date*], [*Pomodori*], [*Bar*], [*Cumulative*],
  ..data.slice(1).map(r => {
    let date = r.at(0)
    let count = int(r.at(1))
    let bar = box(
      width: 1em * count,
      height: 0.6em,
      fill: navy,
    )
    let cum = data.slice(1, data.position-of(r) + 1)
      .map(c => int(c.at(1)))
      .sum()
    (date, str(count), bar, str(cum))
  }).flatten()
)

#v(2em)

== Annotations

#let notes = csv("annotations.csv")

#table(
  columns: (auto, auto),
  stroke: none,
  [*Date*], [*Note*],
  ..notes.slice(1).map(r => (r.at(0), r.at(1))).flatten()
)
```

This file is also available at `doc/export/report.typ`.

Generate the PDF:

```sh
typst compile report.typ report.pdf
```

## Step 4 — Add a chart with Gnuplot

Create `chart.gnuplot`:

```gnuplot
set terminal pdfcairo enhanced font "Helvetica,10"
set output "daily_chart.pdf"

set style data histograms
set style fill solid 0.6 border
set boxwidth 0.7 relative
set key off

set xlabel "Date"
set ylabel "Pomodori finished"
set xtics rotate by -30

plot "daily_counts.csv" using 2:xtic(1) linecolor rgb "navy"
```

This file is also available at `doc/export/chart.gnuplot`.

```sh
gnuplot chart.gnuplot
```

Then add the chart to your Typst report by inserting this section before the Annotations section in `report.typ`:

```typst
== Daily Chart

#image("daily_chart.pdf", width: 100%)
```

Recompile to include it:

```sh
typst compile report.typ report.pdf
```

## Putting it all together

The complete pipeline is captured in `doc/export/export.sh`.

Run it from the `doc/export` directory:

```sh
cd doc/export && bash export.sh
```

The contents of that file:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Weekly pomodoro report — full pipeline
# Usage:  cd doc/export && bash export.sh
# Requires: miller, typst, gnuplot, jq

# 1. Export from rustomato (or use example.csv for testing)
rustomato export --from 2026-04-20 --to 2026-04-26 > export.csv

# 2. Aggregate daily pomodoro counts
# mlr handles CSV quoting correctly (qsv chokes on the annotations JSON column)
mlr --csv filter '$kind == "pomodoro" && $status == "finished"' \
  then put '$date = strftime($started_at, "%Y-%m-%d")' \
  then stats1 -a count -f date -g date \
  then rename date_count,pomodori \
  export.csv > daily_counts.csv

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
```

All files mentioned in this guide live in `doc/export/`:

| File | Purpose |
|---|---|
| `example.csv` | Sample data for offline testing |
| `report.typ` | Typst document template |
| `chart.gnuplot` | Gnuplot histogram script |
| `export.sh` | End-to-end pipeline script |
