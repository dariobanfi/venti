#!/usr/bin/env bash
#
# Brings Venti up on a machine that has Docker and a checkout of this
# repository: writes the settings if there are none, builds the images
# (the server and the web app, both from this checkout) and starts
# everything.
#
#   deploy/setup.sh                     plain HTTP on port 80, for a first look
#   deploy/setup.sh venti.example       the site at https://venti.example
#   deploy/setup.sh --forecasts ...     with the forecast service as well
#                                       (8 GB of memory or more)
#
# Run again, it changes no setting that is already there: it builds what
# has changed and starts it. README.md beside it is the manual.
set -euo pipefail
cd "$(dirname "$0")"

forecasts=no
domain=
for argument in "$@"; do
  case "$argument" in
    --forecasts) forecasts=yes ;;
    -h | --help)
      sed -n '3,14p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "setup.sh: unknown option $argument" >&2
      exit 2
      ;;
    *) domain="$argument" ;;
  esac
done

docker compose version > /dev/null || {
  echo "setup.sh: Docker with the Compose plugin is needed; README.md says how to install it." >&2
  exit 1
}

# Sets one line of .env, without touching the others.
set_line() {
  local name="$1" value="$2" file=.env
  if grep -q "^$name=" "$file"; then
    # Written through a temporary file: `sed -i` differs between systems.
    awk -v name="$name" -v value="$value" 'index($0, name "=") == 1 { print name "=" value; next } { print }' \
      "$file" > "$file.new"
    mv "$file.new" "$file"
  else
    printf '%s=%s\n' "$name" "$value" >> "$file"
  fi
  chmod 600 "$file"
}

if [ ! -f .env ]; then
  cp env.example .env
  chmod 600 .env
  # The database's password is made here and stays in this file.
  set_line POSTGRES_PASSWORD "$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')"
  if [ -n "$domain" ]; then
    # A domain name gets HTTPS by itself, and with it accounts by invite.
    set_line VENTI_SITE_ADDRESS "$domain"
    set_line VENTI_PUBLIC_URL "https://$domain"
    set_line VENTI_REGISTRATION invite
  fi
  echo "setup.sh: wrote deploy/.env"
elif [ -n "$domain" ]; then
  echo "setup.sh: deploy/.env is there already and is left as it is; the domain $domain was not written into it." >&2
fi

if [ "$forecasts" = yes ]; then
  set_line VENTI_OPEN_METEO_URL http://open-meteo:8080
  set_line COMPOSE_PROFILES forecasts
fi

docker compose build
docker compose up --detach

# The server says when it is ready; the data sources follow within seconds.
printf 'setup.sh: waiting for the server'
for _ in $(seq 1 60); do
  if docker compose exec -T server curl --fail --silent --output /dev/null http://127.0.0.1:8080/v1/health; then
    echo
    docker compose ps
    address="$(grep '^VENTI_SITE_ADDRESS=' .env | cut -d= -f2-)"
    echo
    case "$address" in
      :*) echo "Venti is up, in plain HTTP on port ${address#:}. Nothing is encrypted: look, do not create accounts." ;;
      *) echo "Venti is up at https://$address (the certificate takes a moment the first time)."
         echo "Make an invite code for the first pilot with: docker compose exec server venti-server invite" ;;
    esac
    echo "Not done by this script: the backup. README.md, \"Backup\"."
    exit 0
  fi
  printf '.'
  sleep 2
done
echo
echo "setup.sh: the server did not answer within two minutes. Its log:" >&2
docker compose logs --tail 30 server >&2
exit 1
