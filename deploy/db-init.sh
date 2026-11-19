#!/bin/sh
# Makes the role the Venti server signs in as, when the database is first
# created. The PostgreSQL image runs every script in
# /docker-entrypoint-initdb.d once, on an empty data folder, as the
# database's superuser; docker-compose.yml mounts this file there.
#
# Before 2026-11-19 the server signed in as the superuser. Whoever got into
# the server could then read and write files on the database's machine and
# run programs there, which PostgreSQL allows its superuser. The server
# needs none of that: it creates tables and reads and writes rows.
#
#   postgres   the superuser. Has no password after this script, so nothing
#              signs in as it over the network; from inside the container
#              it needs none.
#   venti      owns the database and everything the server creates in it.
#              Not a superuser, creates no databases and no roles. Its
#              password is POSTGRES_PASSWORD from .env.
#
# PostGIS is already in the database: the image's own script put it there
# before this one ran, as the superuser, which is what creating an
# extension takes. The server's first migration asks for it "if it does
# not exist" and finds it.
set -eu

psql --set ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    --set password="$POSTGRES_PASSWORD" --set superuser="$POSTGRES_USER" \
    --set database="$POSTGRES_DB" <<'SQL'
CREATE ROLE venti LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION
    PASSWORD :'password';
ALTER DATABASE :"database" OWNER TO venti;
ALTER ROLE :"superuser" PASSWORD NULL;
SQL
