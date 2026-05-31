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
