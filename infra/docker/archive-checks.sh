# Checks of the WAL archive (ADR-206) shared by wal-archive-test.sh and
# point-in-time-test.sh (ADR-214), sourced after checks.sh (`fail`). The
# archive's folders are the database's (uid 999, 700): they are read as
# root, with sudo unless root. The caller names the folder of the database
# system it reads in `folder`; the stack is reached through `docker compose`
# (COMPOSE_FILE names its files).

as_root() { if ((EUID)); then sudo "$@"; else "$@"; fi; }

# The archived files named <$1>.<64 hex digits>.gz.age in the system's
# folder, $1 a regular expression of the name.
archived() {
  as_root find "$folder" -maxdepth 1 -regextype posix-extended \
    -regex ".*/$1\.[0-9a-f]{64}\.gz\.age" | sort
}

# Waits a minute at most for the file named $1 to be archived; prints it.
await_archived() {
  local found
  for _ in $(seq 1 60); do
    found=$(archived "$1")
    [[ -z $found ]] || {
      echo "$found"
      return
    }
    sleep 1
  done
  fail "$1 was not archived within a minute: $(docker compose exec -T db psql -U mimorya -d mimorya \
    -tAc 'select failed_count, last_failed_wal from pg_stat_archiver')"
}
