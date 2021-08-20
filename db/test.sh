#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

main(){
  reset
  insert-new
  show-new

  reset
  insert-active
  show-active

  reset
  insert-finished
  show-finished

  reset
  insert-cancelled
  show-cancelled

  reset
  new="$(insert-new)"
  start "$new"
  cancel "$new"
  show-all

  reset
  new="$(insert-new)"
  start "$new"
  finish "$new"
  show-all

  reset
  new1="$(insert-new)"
  start "$new1"
  # finish "$new1"
  new2="$(insert-new)"
  start "$new2" # should fail unless $new1 wasn't finished yet
  show-all
}

reset() {
  database < db/schema.sql
}

insert-new() {
  header "${FUNCNAME[0]}"
  uuid=$(generate-uuid)
  echo "INSERT INTO schedulables (uuid) VALUES ('$uuid');" | database
  >&2 echo "Inserted $uuid"
  echo "$uuid"
}

insert-active() {
  header "${FUNCNAME[0]}"
  echo "INSERT INTO schedulables (uuid, started_at) VALUES ('$(generate-uuid)', strftime('%s','now'));" | database
}

insert-finished() {
  header "${FUNCNAME[0]}"
  echo "INSERT INTO schedulables (
            uuid,
            started_at,
            finished_at
        )
        VALUES (
           '$(generate-uuid)',
           strftime('%s', datetime('now','-$((RANDOM % 30 + 1)) minutes')),
           strftime('%s', 'now')
        );" | database
}

insert-cancelled() {
  header "${FUNCNAME[0]}"
  echo "INSERT INTO schedulables (
            uuid,
            started_at,
            cancelled_at
        )
        VALUES (
           '$(generate-uuid)',
           strftime('%s', datetime('now','-$((RANDOM % 30 + 1)) minutes')),
           strftime('%s', 'now')
        );" | database
}

start() {
  local -r uuid="${1:?Argument for uuid missing}"
  header "${FUNCNAME[0]} $uuid"
  echo "UPDATE
          schedulables
        SET
          started_at = strftime('%s','now')
        WHERE
          uuid == '$uuid'
        ;" | database
}

finish() {
  local -r uuid="${1:?Argument for uuid missing}"
  header "${FUNCNAME[0]} $uuid"
  echo "UPDATE
          schedulables
        SET
          finished_at = strftime('%s','now')
        WHERE
          uuid == '$uuid'
        ;" | database
}

cancel() {
  local -r uuid="${1:?Argument for uuid missing}"
  header "${FUNCNAME[0]} $uuid"
  echo "UPDATE
          schedulables
        SET
          cancelled_at = strftime('%s','now')
        WHERE
          uuid == '$uuid'
        ;" | database
}

show-all() {
  header "${FUNCNAME[0]}"
  echo "SELECT
          uuid,
          datetime(started_at, 'unixepoch', 'localtime') as started_at,
          datetime(finished_at, 'unixepoch', 'localtime') as finished_at,
          datetime(cancelled_at, 'unixepoch', 'localtime') as cancelled_at
        FROM
          schedulables
        ;" | database
}

show-new() {
  header "${FUNCNAME[0]}"
  echo "SELECT * from new;" | database
}

show-active() {
  header "${FUNCNAME[0]}"
  echo "SELECT * from active;" | database
}

show-finished() {
  header "${FUNCNAME[0]}"
  echo "SELECT * from finished;" | database
}

show-cancelled() {
  header "${FUNCNAME[0]}"
  echo "SELECT * from cancelled;" | database
}

database() {
  sqlite3 -header -separator ' | ' ~/.rustomato.sqlite3
}

header() {
  >&2 hr
  >&2 echo "$1"
}

generate-uuid() {
  uuidgen | tr "[:upper:]" "[:lower:]"
}

main "$@"
