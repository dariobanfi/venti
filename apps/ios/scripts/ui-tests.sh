#!/usr/bin/env bash
# Runs the iPhone app's UI tests on this Mac, on several simulators at once:
# the app is built once, and the tests are dealt out among the simulators as
# the CI workflow deals them out among its machines (every test function,
# sorted by name, in turn). Each simulator has its own pair of servers: the
# development server on recorded data and, in front of it, the stand-in with
# a made-up day (tools/made-up-day/serve.py).
#
#   apps/ios/scripts/ui-tests.sh                      all UI tests, on 3 simulators
#   apps/ios/scripts/ui-tests.sh testA testB          only these
#   apps/ios/scripts/ui-tests.sh -n 4 --unit          on 4, and the unit tests first
#   apps/ios/scripts/ui-tests.sh --first "iPhone 17e" testA   that simulator first
#
# Run it from anywhere, with the core, the design pack and the project made
# (build-core.sh, build-design.sh, `xcodegen generate`) and the server built
# (`cargo build -p venti-server`). Everything it makes is under
# target/apple/ui-tests/<run>/: a result bundle and a log for each part, the
# servers' logs, and the screenshots of all parts in screenshots/. It ends
# with one line for each test and exits with 1 if any test failed.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"

# The simulators to use, by name, in this order: each of these on the newest
# iOS, then the same on an older one. Not "iPhone 18 Pro": the owner looks at
# that one. "iPhone 18 Pro Max" comes last, as the largest.
names=("iPhone 17" "iPhone 17e" "iPhone Air" "iPhone 18 Pro Max")
# And never this device, whatever it is called.
spared="9417DD04-2618-4C78-9C3E-9BA8CA21DB88"
# The ports of the first part's two servers; each further part takes the
# next two. Kept clear of 8088 and of 8190 and above, which others use.
first_port=8090
most=8

count=3
unit=false
tests=()
while [ $# -gt 0 ]; do
  case "$1" in
    -n|--simulators) count="$2"; shift 2 ;;
    --unit) unit=true; shift ;;
    --first)
      # Never the owner's simulator, by any route.
      [ "$2" = "iPhone 18 Pro" ] && { echo "not the iPhone 18 Pro: the owner looks at it" >&2; exit 2; }
      names=("$2" "${names[@]}"); shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    test*) tests+=("$1"); shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
if ! [[ "$count" =~ ^[0-9]+$ ]] || [ "$count" -lt 1 ] || [ "$count" -gt "$most" ]; then
  echo "the number of simulators is 1 to $most" >&2
  exit 2
fi

# The tests to run, sorted by name: all of them, or those named.
if [ ${#tests[@]} -eq 0 ]; then
  while read -r name; do tests+=("$name"); done < <(
    grep -oE 'func test[A-Za-z0-9_]+' apps/ios/VentiUITests/VentiUITests.swift | sed 's/^func //' | LC_ALL=C sort
  )
else
  named=("${tests[@]}")
  tests=()
  while read -r name; do tests+=("$name"); done < <(printf '%s\n' "${named[@]}" | LC_ALL=C sort -u)
fi
[ "$count" -gt ${#tests[@]} ] && count=${#tests[@]}

run="$(date +%Y%m%d-%H%M%S)"
out="target/apple/ui-tests/$run"
mkdir -p "$out"
started=$(date +%s)

# Whatever happens, the servers this script started are stopped.
pids=()
stop_servers() {
  for pid in ${pids[@]+"${pids[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in ${pids[@]+"${pids[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
}
trap stop_servers EXIT
trap 'echo "interrupted" >&2; exit 130' INT TERM

# The simulators: by the names above, the newest iOS first, as many as asked.
simulators=()
while read -r line; do simulators+=("$line"); done < <(xcrun simctl list devices available -j | python3 -c '
import json, re, sys
names, spared, count = sys.argv[1].split("|"), sys.argv[2], int(sys.argv[3])
found = []
for runtime, devices in json.load(sys.stdin)["devices"].items():
    match = re.search(r"iOS-(\d+)-(\d+)", runtime)
    if not match:
        continue
    version = (int(match.group(1)), int(match.group(2)))
    for device in devices:
        if device["name"] in names and device["udid"] != spared:
            found.append((names.index(device["name"]), tuple(-v for v in version), device["udid"], device["name"]))
# Each name on the newest iOS, then on older ones; the last name after all.
last = len(names) - 1
found.sort(key=lambda d: (d[0] == last, d[1], d[0]))
for d in found[:count]:
    print(d[2] + "|" + d[3])
' "$(IFS='|'; echo "${names[*]}")" "$spared" "$count")
if [ ${#simulators[@]} -lt "$count" ]; then
  echo "only ${#simulators[@]} of the simulators named in this script are installed" >&2
  exit 1
fi

# Booted, and left booted for the next run.
for simulator in "${simulators[@]}"; do
  udid="${simulator%%|*}"
  xcrun simctl boot "$udid" 2>/dev/null || true
done
for simulator in "${simulators[@]}"; do
  xcrun simctl bootstatus "${simulator%%|*}" -b > /dev/null
done

# Built once, for every simulator: for this Mac's processor alone, which is
# all the core is built for. The app is signed for this Mac only.
echo "building ($out/build.log)"
if ! xcodebuild build-for-testing \
  -project apps/ios/Venti.xcodeproj -scheme Venti \
  -destination "generic/platform=iOS Simulator" \
  -derivedDataPath target/apple/DerivedData \
  ARCHS=arm64 CODE_SIGN_IDENTITY=- > "$out/build.log" 2>&1; then
  grep -E "error: " "$out/build.log" | head -20 >&2
  echo "the build failed: $out/build.log" >&2
  exit 1
fi

# The unit tests, if asked for, on the first simulator before the rest.
failed=0
if $unit; then
  echo "unit tests ($out/unit.log)"
  if xcodebuild test-without-building \
    -project apps/ios/Venti.xcodeproj -scheme Venti \
    -destination "platform=iOS Simulator,id=${simulators[0]%%|*}" \
    -derivedDataPath target/apple/DerivedData \
    -resultBundlePath "$out/unit.xcresult" \
    -only-testing:VentiTests > "$out/unit.log" 2>&1; then
    grep -E "Executed [0-9]+ tests" "$out/unit.log" | tail -1
  else
    grep -E "error: |Executed [0-9]+ tests" "$out/unit.log" | tail -10
    failed=1
  fi
  # The pictures the unit tests keep (the home-screen widget's face, the
  # radar's strip) go beside the UI tests' screenshots.
  if [ -d "$out/unit.xcresult" ]; then
    xcrun xcresulttool export attachments --path "$out/unit.xcresult" --output-path "$out/attachments-unit" > /dev/null
  fi
fi

# Waits until a server answers its health check.
answers() {
  for _ in $(seq 1 60); do
    curl -sf "http://127.0.0.1:$1/v1/health" > /dev/null && return 0
    sleep 0.5
  done
  echo "nothing answers on port $1: see $out" >&2
  return 1
}

# Each part: its servers, then its tests, in the background.
parts=()
for part in $(seq 1 "$count"); do
  simulator="${simulators[$((part - 1))]}"
  udid="${simulator%%|*}"
  server=$((first_port + 2 * (part - 1)))
  standin=$((server + 1))
  for port in $server $standin; do
    if curl -s -o /dev/null "http://127.0.0.1:$port/"; then
      echo "port $port is taken: something else listens there" >&2
      exit 1
    fi
  done
  target/debug/venti-server dev --listen "127.0.0.1:$server" --maps apps/ios/Venti/Resources \
    > "$out/server-$part.log" 2>&1 &
  pids+=($!)
  python3 tools/made-up-day/serve.py --behind "http://127.0.0.1:$server" --port "$standin" \
    > "$out/made-up-day-$part.log" 2>&1 &
  pids+=($!)
  answers "$server"
  answers "$standin"

  # This part's tests: every count-th, as the workflow deals them.
  only=()
  for i in "${!tests[@]}"; do
    if [ $((i % count)) -eq $((part - 1)) ]; then
      only+=("-only-testing:VentiUITests/VentiUITests/${tests[$i]}")
    fi
  done
  echo "part $part: ${#only[@]} tests on ${simulator#*|} ($udid), servers on $server and $standin"
  # A test that fails is run once more in a fresh process, as in the
  # workflow: a simulator now and then fails to launch the app.
  TEST_RUNNER_VENTI_SERVER="http://127.0.0.1:$server" \
  TEST_RUNNER_VENTI_MADE_UP_DAY_SERVER="http://127.0.0.1:$standin" \
  xcodebuild test-without-building \
    -project apps/ios/Venti.xcodeproj -scheme Venti \
    -destination "platform=iOS Simulator,id=$udid" \
    -derivedDataPath target/apple/DerivedData \
    -resultBundlePath "$out/part-$part.xcresult" \
    "${only[@]}" \
    -retry-tests-on-failure -test-iterations 2 \
    -test-repetition-relaunch-enabled YES \
    > "$out/part-$part.log" 2>&1 &
  parts+=($!)
done

for pid in ${parts[@]+"${parts[@]}"}; do
  wait "$pid" || failed=1
done
stop_servers
pids=()

# The screenshots of all parts in one folder, under their own names, and one
# line for each test.
mkdir -p "$out/screenshots"
for part in $(seq 1 "$count"); do
  bundle="$out/part-$part.xcresult"
  [ -d "$bundle" ] || { echo "part $part left no results: $out/part-$part.log"; failed=1; continue; }
  rm -rf "$out/attachments-$part"
  xcrun xcresulttool export attachments --path "$bundle" --output-path "$out/attachments-$part" > /dev/null
  xcrun xcresulttool get test-results tests --path "$bundle" > "$out/tests-$part.json"
done
python3 - "$out" "$count" "$(printf '%s|' "${simulators[@]}")" <<'EOF' || failed=1
import json, pathlib, re, shutil, sys

out, count = pathlib.Path(sys.argv[1]), int(sys.argv[2])
names = sys.argv[3].strip("|").split("|")[1::2]
target = out / "screenshots"
lines, failures = [], 0
for part in ["unit"] + list(range(1, count + 1)):
    source = out / f"attachments-{part}"
    if (source / "manifest.json").exists():
        for test in json.loads((source / "manifest.json").read_text()):
            for attachment in test.get("attachments", []):
                exported = attachment["exportedFileName"]
                name = attachment.get("suggestedHumanReadableName") or exported
                # "light-1-map_0_<UUID>.png" becomes "light-1-map.png".
                name = re.sub(r"_\d+_[0-9A-Fa-f-]{36}(\.\w+)$", r"\1", name)
                if name.endswith(".png"):
                    shutil.copy(source / exported, target / name)
    results = out / f"tests-{part}.json"
    if not results.exists():
        continue

    def cases(node):
        if node.get("nodeType") == "Test Case":
            yield node
        for child in node.get("children", []):
            yield from cases(child)

    for root in json.loads(results.read_text()).get("testNodes", []):
        for case in cases(root):
            result = case.get("result", "?")
            failures += result != "Passed"
            lines.append(
                f"{result.lower():8} {case.get('durationInSeconds', 0):6.0f} s  "
                f"{case['name'].removesuffix('()'):55} part {part}, {names[part - 1]}"
            )
for line in sorted(lines, key=lambda l: l.split()[2]):
    print(line)
sys.exit(1 if failures else 0)
EOF

echo "screenshots: $out/screenshots ($(ls "$out/screenshots" | wc -l | tr -d ' '))"
echo "wall time: $(( ($(date +%s) - started) / 60 )) min $(( ($(date +%s) - started) % 60 )) s"
exit $failed
