#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

main(){
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
  uuid="$(insert-active)"
  cancel "$uuid"
  show-all

  reset
  uuid="$(insert-active)"
  finish "$uuid"
  show-all

  reset
  uuid="$(insert-active)"
  finish "$uuid"
  insert-active # must fail if $uuid was not finished yet
  show-all

  reset
  insert-two-pids # should fail
}

reset() {
  database < db/schema.sql
}

insert-active() {
  header "${FUNCNAME[0]}"
  uuid=$(generate-uuid)
  echo "INSERT INTO schedulables (pid, uuid, duration, started_at) VALUES ($RANDOM,'$uuid', 25, strftime('%s','now'));" | database
  >&2 echo "Inserted $uuid"
  echo "$uuid"
}

insert-two-pids() {
  header "${FUNCNAME[0]}"
  uuid=$(generate-uuid)
  echo "INSERT INTO schedulables (pid, uuid, duration, started_at) VALUES ($RANDOM,'$uuid', 25, strftime('%s','now'));" | database
  uuid=$(generate-uuid)
  echo "INSERT INTO schedulables (pid, uuid, duration, started_at) VALUES ($RANDOM,'$uuid', 25, strftime('%s','now'));" | database
}

insert-finished() {
  header "${FUNCNAME[0]}"
  echo "INSERT INTO schedulables (
            uuid,
            duration,
            started_at,
            finished_at
        )
        VALUES (
           '$(generate-uuid)',
           25,
           strftime('%s', datetime('now','-$((RANDOM % 30 + 1)) minutes')),
           strftime('%s', 'now')
        );" | database
}

insert-cancelled() {
  header "${FUNCNAME[0]}"
  echo "INSERT INTO schedulables (
            uuid,
            duration,
            started_at,
            cancelled_at
        )
        VALUES (
           '$(generate-uuid)',
           25,
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
          pid,
          kind,
          uuid,
          datetime(started_at, 'unixepoch', 'localtime') as started_at,
          datetime(finished_at, 'unixepoch', 'localtime') as finished_at,
          datetime(cancelled_at, 'unixepoch', 'localtime') as cancelled_at
        FROM
          schedulables
        ;" | database
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
  sqlite3 -header -separator ' | ' ~/.rustomato.db
}

header() {
  >&2 hr
  >&2 echo "$1"
}

generate-uuid() {
  uuidgen | tr "[:upper:]" "[:lower:]"
}

main "$@"
