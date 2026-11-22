#!/usr/bin/env bash
# Runs the Android app's UI tests on this Mac, on several emulators at once.
# The app and its tests are built once; each emulator gets them installed
# and then takes the next test from one shared queue until none is left, the
# longest first as the last run timed them. Each emulator has its own pair of
# servers: the development server on recorded data and, in front of it, the
# stand-in with a made-up day (tools/made-up-day/serve.py).
#
#   apps/android/scripts/ui-tests.sh                        all UI tests, on 3 emulators
#   apps/android/scripts/ui-tests.sh theCockpitWarnsOfAirspace theMapNamesPlaces
#   apps/android/scripts/ui-tests.sh -n 4 --stop            on 4, and stop them at the end
#   apps/android/scripts/ui-tests.sh --no-build theMapNamesPlaces
#
# A test is named by its function (VentiUiTest's), or as Class#function for
# another class, or by its class alone. Run it from anywhere, with the
# server built (`cargo build -p venti-server`). The emulators are the
# virtual devices pixel6-api36-b, -c, -d and -e on the ports 5556, 5558,
# 5560 and 5562; those missing are made as copies of pixel6-api36-b. They
# are left running for the next run unless --stop is given. emulator-5554
# is the owner's and is never touched. Everything the run makes is under
# target/android/ui-tests/<run>/: a log for each test, the servers' and
# emulators' logs, and every screenshot in screenshots/. It ends with one
# line for each test and exits with 1 if any test failed.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"

export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21}"
# rustup's tools before Homebrew's cargo: Gradle builds the core itself.
export PATH="/opt/homebrew/opt/rustup/bin:$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"

# The virtual devices to use, in this order, and the port of each.
avds=(pixel6-api36-b pixel6-api36-c pixel6-api36-d pixel6-api36-e)
ports=(5556 5558 5560 5562)
# The device the others are copied from.
model="pixel6-api36-b"
# Never this one: the owner's.
spared="emulator-5554"
# The ports of the first emulator's two servers; each further one takes the
# next two. Clear of 8088, of 8090 to 8095 (the iPhone's tests) and of 8290.
first_port=8190

app_id="fly.venti.app"
test_id="fly.venti.app.test"
runner="$test_id/androidx.test.runner.AndroidJUnitRunner"
# Where the tests keep their screenshots without Gradle: the app's own
# folder on the device's shared storage (TestDevice.kept).
shots_on_device="/sdcard/Android/data/$app_id/files/screenshots"

count=3
stop=false
build=true
named=()
while [ $# -gt 0 ]; do
  case "$1" in
    -n|--emulators) count="$2"; shift 2 ;;
    --stop) stop=true; shift ;;
    --no-build) build=false; shift ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) named+=("$1"); shift ;;
  esac
done
if ! [[ "$count" =~ ^[0-9]+$ ]] || [ "$count" -lt 1 ] || [ "$count" -gt ${#avds[@]} ]; then
  echo "the number of emulators is 1 to ${#avds[@]}" >&2
  exit 2
fi
if [ ! -x target/debug/venti-server ]; then
  echo "Build the server first: cargo build -p venti-server" >&2
  exit 1
fi

run="$(date +%Y%m%d-%H%M%S)"
out="target/android/ui-tests/$run"
mkdir -p "$out/logs" "$out/screenshots"
started=$(date +%s)
# How long each test took the last time, for the order of the queue.
timings="target/android/ui-tests/timings.txt"

# Whatever happens, what this script started is stopped: the servers and the
# workers always, the emulators only when asked to.
pids=()
serials=()
cleanup() {
  for pid in ${pids[@]+"${pids[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in ${pids[@]+"${pids[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  # A test still under way on an emulator ends with the run.
  for serial in ${serials[@]+"${serials[@]}"}; do
    [ "$serial" = "$spared" ] && continue
    adb -s "$serial" shell am force-stop "$app_id" >/dev/null 2>&1 || true
  done
  if $stop; then
    for serial in ${serials[@]+"${serials[@]}"}; do
      [ "$serial" = "$spared" ] && continue
      adb -s "$serial" emu kill >/dev/null 2>&1 || true
    done
  fi
}
trap cleanup EXIT
trap 'echo "interrupted" >&2; exit 130' INT TERM

# --- The build, while the emulators start. ---------------------------------

if $build; then
  echo "Building the app and its tests…"
  (cd apps/android && ./gradlew -q :app:assembleDebug :app:assembleDebugAndroidTest -Pventi.abis=arm64-v8a) \
    >"$out/logs/build.log" 2>&1 &
  build_pid=$!
  pids+=("$build_pid")
fi

# --- The emulators. --------------------------------------------------------

# The serial a running virtual device has, if it runs.
serial_of() {
  local serial
  for serial in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
    [ "$serial" = "$spared" ] && continue
    if [ "$(adb -s "$serial" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$1" ]; then
      echo "$serial"
      return
    fi
  done
}

# A copy of the model device: the same system image and settings, its own data.
make_avd() {
  local name="$1"
  local image
  image="$(sed -n 's/^image.sysdir.1=system-images\/\(.*\)\/$/\1/p' "$HOME/.android/avd/$model.avd/config.ini" | tr '/' ';')"
  echo "Making the virtual device $name…"
  echo no | avdmanager create avd -n "$name" -k "system-images;$image" -d pixel_6 >"$out/logs/avd-$name.log" 2>&1
  sed "s/$model/$name/g" "$HOME/.android/avd/$model.avd/config.ini" >"$HOME/.android/avd/$name.avd/config.ini"
}

for i in $(seq 0 $((count - 1))); do
  name="${avds[$i]}"
  [ -d "$HOME/.android/avd/$name.avd" ] || make_avd "$name"
  serial="$(serial_of "$name")"
  if [ -z "$serial" ]; then
    serial="emulator-${ports[$i]}"
    echo "Starting $name on ${ports[$i]}…"
    # Detached, so that it outlives the script. It boots from its snapshot
    # where it has one, and saves one when it is stopped.
    nohup emulator -avd "$name" -port "${ports[$i]}" -no-window -gpu host -noaudio -no-boot-anim -no-metrics \
      >"$out/logs/emulator-$name.log" 2>&1 &
    disown
  fi
  serials+=("$serial")
done

for serial in "${serials[@]}"; do
  waited=0
  until [ "$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
    sleep 2
    waited=$((waited + 2))
    if [ "$waited" -gt 600 ]; then
      echo "$serial did not finish starting in ten minutes" >&2
      exit 1
    fi
  done
  # As Gradle's testOptions do: no animations to wait for.
  for scale in window_animation_scale transition_animation_scale animator_duration_scale; do
    adb -s "$serial" shell settings put global "$scale" 0
  done
  echo "$serial is up"
done

if $build; then
  if ! wait "$build_pid"; then
    cat "$out/logs/build.log" >&2
    echo "The build failed." >&2
    exit 1
  fi
fi
app_apk="apps/android/app/build/outputs/apk/debug/app-debug.apk"
test_apk="apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"

# --- The servers and the app on each emulator. ------------------------------

for i in $(seq 0 $((count - 1))); do
  serial="${serials[$i]}"
  port=$((first_port + 2 * i))
  target/debug/venti-server dev --listen "127.0.0.1:$port" --maps apps/ios/Venti/Resources \
    >"$out/logs/server-$port.log" 2>&1 &
  pids+=("$!")
  python3 tools/made-up-day/serve.py --behind "http://127.0.0.1:$port" --port $((port + 1)) \
    >"$out/logs/server-$((port + 1)).log" 2>&1 &
  pids+=("$!")
  # Installed afresh, as Gradle does: nothing of an earlier run is left.
  (
    adb -s "$serial" uninstall "$test_id" >/dev/null 2>&1 || true
    adb -s "$serial" uninstall "$app_id" >/dev/null 2>&1 || true
    adb -s "$serial" install -t "$app_apk" >/dev/null
    adb -s "$serial" install -t "$test_apk" >/dev/null
    adb -s "$serial" shell rm -rf "$shots_on_device"
  ) &
  pids+=("$!")
  install_pids[$i]=$!
done
for i in $(seq 0 $((count - 1))); do
  wait "${install_pids[$i]}" || { echo "Installing on ${serials[$i]} failed." >&2; exit 1; }
done
for i in $(seq 0 $((count - 1))); do
  port=$((first_port + 2 * i))
  for p in "$port" $((port + 1)); do
    waited=0
    until curl -s -o /dev/null "http://127.0.0.1:$p/v1/health"; do
      sleep 1
      waited=$((waited + 1))
      [ "$waited" -gt 60 ] && { echo "The server on $p did not start; see $out/logs/server-$p.log" >&2; exit 1; }
    done
  done
done

# --- The tests, in a queue. ------------------------------------------------

# As the runner names them: Class#function, or a class with all its tests.
# A name with a capital first letter and no package is a class of the app's
# tests (LiveFlightTest, LiveFlightTest#aFlightRecordsItself), found by its
# file under app/src/androidTest.
full_name() {
  local class file package
  case "$1" in
    *.*) echo "$1" ;;
    [A-Z]*)
      class="${1%%#*}"
      file="$(find apps/android/app/src/androidTest -name "$class.kt" | head -1)"
      package="$(sed -n 's/^package \([a-z.]*\).*/\1/p' "$file" 2>/dev/null | head -1)"
      echo "${package:-fly.venti.app}.$1"
      ;;
    *) echo "fly.venti.app.VentiUiTest#$1" ;;
  esac
}

queue="$out/queue"
if [ ${#named[@]} -eq 0 ]; then
  # Every test the instrumentation has, as the runner lists them without
  # running them.
  adb -s "${serials[0]}" shell am instrument -w -r -e log true "$runner" | tr -d '\r' |
    awk -F= '/^INSTRUMENTATION_STATUS: class=/ { c = $2 } /^INSTRUMENTATION_STATUS: test=/ { print c "#" $2 }' |
    LC_ALL=C sort -u >"$out/all"
else
  for name in "${named[@]}"; do full_name "$name"; done | LC_ALL=C sort -u >"$out/all"
fi
# The longest first, as timed last time; one never timed before them all.
while read -r test; do
  seconds="$(awk -v t="$test" '$1 == t { s = $2 } END { print s + 0 }' "$timings" 2>/dev/null || echo 0)"
  [ "$seconds" = "0" ] && seconds=100000
  echo "$seconds $test"
done <"$out/all" | sort -rn | awk '{ print $2 }' >"$queue"
total=$(wc -l <"$queue" | tr -d ' ')
[ "$total" -gt 0 ] || { echo "No tests to run." >&2; exit 2; }
echo "Running $total tests on ${#serials[@]} emulators…"

# The next test from the queue, or nothing when it is empty.
take() {
  until mkdir "$queue.lock" 2>/dev/null; do sleep 0.1; done
  head -1 "$queue"
  tail -n +2 "$queue" >"$queue.rest"
  mv "$queue.rest" "$queue"
  rmdir "$queue.lock"
}

# One emulator works through the queue: each test in an instrumentation of
# its own, against this emulator's servers.
work() {
  local serial="$1" port="$2"
  local test name begun seconds result
  while true; do
    test="$(take)"
    [ -n "$test" ] || break
    name="${test##*#}"
    begun=$(date +%s)
    adb -s "$serial" shell am instrument -w -r --no-window-animation \
      -e class "$test" \
      -e ventiServer "http://10.0.2.2:$port" \
      -e madeUpDayServer "http://10.0.2.2:$((port + 1))" \
      "$runner" >"$out/logs/$name.log" 2>&1 || true
    seconds=$(($(date +%s) - begun))
    if grep -q "^INSTRUMENTATION_STATUS_CODE: -4" "$out/logs/$name.log"; then
      result="skipped"
    # "OK (1 test)", or "OK (2 tests)" for a class named alone.
    elif grep -Eq "^OK \([0-9]+ tests?\)" "$out/logs/$name.log"; then
      result="passed"
    else
      result="FAILED"
    fi
    printf '%-58s %-7s %4ss  %s\n' "$name" "$result" "$seconds" "$serial" >>"$out/results"
    printf '%-58s %-7s %4ss  %s\n' "$name" "$result" "$seconds" "$serial"
    echo "$test $seconds" >>"$out/timed"
  done
  # Every screenshot this emulator's tests kept.
  adb -s "$serial" pull "$shots_on_device/." "$out/screenshots/" >/dev/null 2>&1 || true
}

: >"$out/results"
: >"$out/timed"
workers=()
for i in $(seq 0 $((count - 1))); do
  work "${serials[$i]}" $((first_port + 2 * i)) &
  pids+=("$!")
  workers+=("$!")
done
for pid in "${workers[@]}"; do
  wait "$pid" || true
done

# The times of this run replace those of the tests it ran.
if [ -f "$timings" ]; then
  awk 'NR == FNR { ran[$1] = 1; next } !($1 in ran)' "$out/timed" "$timings" >"$timings.new" || true
fi
cat "$out/timed" >>"$timings.new"
mv "$timings.new" "$timings"

# --- What came of it. ------------------------------------------------------

echo
LC_ALL=C sort "$out/results"
passed=$(grep -c " passed " "$out/results" || true)
skipped=$(grep -c " skipped " "$out/results" || true)
failed=$(grep -c " FAILED " "$out/results" || true)
wall=$(($(date +%s) - started))
echo
echo "$passed passed, $failed failed, $skipped skipped of $total, in $((wall / 60)) min $((wall % 60)) s on ${#serials[@]} emulators."
echo "Screenshots: $out/screenshots ($(ls "$out/screenshots" | wc -l | tr -d ' '))"
echo "Logs: $out/logs"
if $stop; then
  echo "The emulators are stopped."
else
  echo "Left running: ${serials[*]}"
fi
[ "$failed" -eq 0 ]
