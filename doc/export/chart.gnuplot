set terminal svg enhanced font "Helvetica,10"
set output "daily_chart.svg"

set datafile separator comma

set style data histograms
set style fill solid 0.6 border
set boxwidth 0.7 relative
set key off

set xlabel "Date"
set ylabel "Pomodori finished"
set xtics rotate by -30

plot "daily_counts.csv" using 2:xtic(1) linecolor rgb "navy"
