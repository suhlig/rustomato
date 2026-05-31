# Annotations

Annotations let you attach arbitrary text to a pomodoro or break. This is useful for noting what you worked on, capturing thoughts mid-session, or tagging entries for later review.

# Interactive annotation with `sk`

For users who want to select a pomodoro interactively with a fuzzy-finder preview before annotating, install [skim](https://github.com/skim-rs/skim) and add this shell function:

```sh
rustomato-annotate() {
  local target
  target=$(
    rustomato list --no-header \
      | sk --delimiter ' ' --with-nth 1 \
           --preview 'rustomato show {1}' \
           --layout=reverse \
      | cut -d' ' -f1
  ) && rustomato pomodoro annotate --target "$target" "$@"
}
```

What this does:

| Step | Description |
|---|---|
| `rustomato list --no-header` | List recent entries, one per line, no header |
| `sk --delimiter ' ' --with-nth 1` | Show only the UUID column; `{1}` in preview refers to the UUID |
| `sk --preview 'rustomato show {1}'` | Show full details of the highlighted entry |
| `cut -d' ' -f1` | Extract the UUID from the selected line |
| `rustomato annotate --target "$target"` | Annotate with the chosen target |

The preview window shows the full details of each entry as you arrow through the list. When you press enter, the annotation is applied to the selected pomodoro.
