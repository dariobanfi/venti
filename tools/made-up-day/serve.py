#!/usr/bin/env python3
"""A Venti server whose forecast is made up: a strong summer day, today.

For UI tests and their screenshots, and for nothing else. The development
server (`venti-server dev`) answers forecasts from what a weather model
said on two days in late November: rain, cloud and nothing to climb in.
The screens that show a cross-country day or a big day cannot be seen
with it. This program stands in front of a development server and hands
on everything that server answers, except `GET /v1/forecast`: there it
keeps the real answer's place, grid cell, model ground and credit, and
puts made-up hours in place of the model's, from this midnight (UTC) on
for two days: 26 °C under cumulus at 3500 m with a light south wind, and
by day as much heat from the ground as a summer noon gives.

How much heat depends on the place asked for, so that a list of take-offs
shows more than one kind of day: 550, 300 or 70 W/m² at noon, which the
apps' core reads as a big day, a cross-country day and local thermals.

It also answers `GET /v1/thermals` with a made-up thermal map in the
server's wire format, so that a map of the kind of day shows every kind:
a big day in the north-west, a cross-country day beside it, local
thermals in the south-west, weak ones beside those, a marginal band in the
east (by gusts) and a corner there that is not flyable (by rain), as
patches of whole cells.
Three days from today by the development server's clock (the `time` of its
health answer, which is the clock the apps name "Today" by; its recorded
forecasts are of other days), each a step weaker everywhere than the day
before, with climbs and tops that fit each kind. The grid is a small one of its own,
five rows by nine columns of a tenth of a degree from 47.5 N 11.5 E,
unless the development server's map has a grid of 30 cells or more (the
one recorded has four, too few for six patches); the source, model and
credit are the development server's where it has a map. Every value of
the map is made up, deterministically: the same on every start.

None of it is a forecast. The answer says so in the name of its model's
run only by being impossible for the season; whoever shows a screenshot
made with it says that the day is made up.

    serve.py --behind http://127.0.0.1:8080 --port 8081
"""

import argparse
import datetime
import json
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# What the ground gives to the air at noon, W/m², by the place asked for.
HEAT_W_M2 = (550.0, 300.0, 70.0)
# How much of that each hour of the day (UTC) has: a sun that is up from
# five to five, strongest from ten to noon. Wide enough to hold a November
# day's light in the Alps with most of its strength.
SUN = {5: 0.3, 6: 0.5, 7: 0.65, 8: 0.8, 9: 0.9, 10: 1.0, 11: 1.0, 12: 1.0, 13: 0.95, 14: 0.85,
       15: 0.7, 16: 0.5, 17: 0.3}
HANDED_BACK = ("Content-Type", "Cache-Control", "ETag", "Last-Modified", "Vary", "Retry-After")
PASSED_ON = ("Authorization", "Content-Type", "Accept", "If-None-Match")


def hour(time, heat_at_noon, ground_m):
    """One made-up hour over a cell whose ground lies at `ground_m`."""
    sun = SUN.get(time.hour, 0.0)
    warmth = 14.0 + 12.0 * sun
    made = {
        "time": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "wind_speed_ms": 2.0,
        "wind_direction_deg": 180.0,
        "wind_gust_ms": 4.0,
        "temperature_c": warmth,
        "dew_point_c": 10.0,
        "cloud_cover_pct": 25.0 * sun,
        "cloud_cover_low_pct": 25.0 * sun,
        "cloud_cover_mid_pct": 0.0,
        "cloud_cover_high_pct": 0.0,
        "precipitation_mm": 0.0,
        "cape_j_kg": 150.0,
        "freezing_level_m": 4200.0,
        "updraft_ms": 0.5,
        # By night the air warms the ground, as under a clear sky.
        "heat_to_air_w_m2": heat_at_noon * sun if sun > 0 else -30.0,
        # Well mixed to 700 m above the ground, and a layer above it that
        # stops air risen from the ground some 2000 m up.
        "winds_aloft": [
            {"above_ground_m": 80.0, "wind_speed_ms": 3.0, "wind_direction_deg": 180.0,
             "temperature_c": warmth - 2.0},
            {"above_ground_m": 180.0, "wind_speed_ms": 3.0, "wind_direction_deg": 180.0,
             "temperature_c": warmth - 3.0},
            {"pressure_hpa": 850.0, "altitude_m": ground_m + 700.0, "wind_speed_ms": 3.5,
             "wind_direction_deg": 190.0, "temperature_c": warmth - 8.0},
            {"pressure_hpa": 700.0, "altitude_m": ground_m + 2600.0, "wind_speed_ms": 4.0,
             "wind_direction_deg": 200.0, "temperature_c": warmth - 25.0},
            {"pressure_hpa": 600.0, "altitude_m": ground_m + 3500.0, "wind_speed_ms": 5.0,
             "wind_direction_deg": 210.0, "temperature_c": warmth - 30.0},
        ],
    }
    if sun >= 0.5:
        # The base rises with the day's warmth, as it does.
        made["convective_cloud_base_m"] = 2900.0 + 600.0 * sun
        made["convective_cloud_top_m"] = 4200.0
    return made


def made_up(answer, lat, lon, now):
    """The real answer with made-up hours in place of the model's."""
    midnight = now.replace(hour=0, minute=0, second=0, microsecond=0)
    # By the thousandths of the place's degrees: neighbours differ.
    heat = HEAT_W_M2[(round(lat * 1000) + round(lon * 1000)) % len(HEAT_W_M2)]
    stamp = lambda time: time.strftime("%Y-%m-%dT%H:%M:%SZ")
    answer["fetched_at"] = stamp(now)
    for layer in answer.get("layers", []):
        # One model is enough: the finest of those the server answered.
        series = [s for s in layer.get("series", []) if s.get("model") == "icon-d2"][:1]
        for one in series:
            ground = one.get("cell_elevation_m") or 800.0
            # A run that began three hours before this midnight and was
            # ready an hour later, as a real one would be.
            run = midnight - datetime.timedelta(hours=3)
            one["run"] = stamp(run)
            one["available_at"] = stamp(run + datetime.timedelta(minutes=70))
            one["hours"] = [
                hour(midnight + datetime.timedelta(hours=ahead), heat, ground)
                for ahead in range(46)
            ]
        layer["series"] = series
    return answer


# The kinds of day of the made-up map, with the climb (m/s) and the height
# the thermals reach (m above mean sea level) of each, and whether that
# height is a base of cumulus.
KINDS = {
    "big": (2.4, 3500, True),
    "cross_country": (1.6, 2900, True),
    "local": (0.8, 2100, False),
    "weak": (0.3, 1500, False),
}
WEAKER = {"big": "cross_country", "cross_country": "local", "local": "weak", "weak": "weak"}
# The grid the made-up map has of its own, and the least number of cells
# a development server's grid needs to be used in its place.
OWN_GRID = {"south": 47.5, "west": 11.5, "step_deg": 0.1, "rows": 5, "columns": 9}
LEAST_CELLS = 30


def patch(row, column, rows, columns):
    """The verdict and the kind of day of a cell, by where it lies."""
    north = row >= rows * 0.6
    across = column / max(columns - 1, 1)
    if across >= 0.85:
        return ("not_flyable", None) if row < rows * 0.4 else ("marginal", "local")
    if across >= 0.7:
        return ("marginal", "local")
    if across >= 0.35:
        return ("flyable", "cross_country" if north else "weak")
    return ("flyable", "big" if north else "local")


def made_up_map(real, today=None):
    """A made-up thermal map, on the grid of `real` if it has a big enough
    one, with its source, model and credit; `real` may be None. Its days
    begin at `today`, or on the day `real` was computed without one."""
    real = real or {}
    grid = real.get("grid")
    if not grid or grid["rows"] * grid["columns"] < LEAST_CELLS:
        grid = dict(OWN_GRID)
        ground = [700 + 150 * ((row * 2 + column * 3) % 5)
                  for row in range(grid["rows"]) for column in range(grid["columns"])]
    else:
        ground = [height if height is not None else 800 for height in real.get("ground_m", [])]
    rows, columns = grid["rows"], grid["columns"]
    computed = real.get("computed_at") or "2026-11-21T03:25:00Z"
    today = today or datetime.date.fromisoformat(computed[:10])
    days = []
    for ahead in range(3):
        verdicts, whys, kinds, climbs, tops, clouds = [], [], [], [], [], []
        for row in range(rows):
            for column in range(columns):
                verdict, kind = patch(row, column, rows, columns)
                for _ in range(ahead):
                    kind = WEAKER[kind] if kind is not None else None
                climb, top, cloud = KINDS[kind] if kind else (None, None, False)
                verdicts.append(verdict)
                # Rain in the corner that is not flyable, gusts where the
                # day is marginal.
                whys.append({"not_flyable": "rain", "marginal": "gusts"}.get(verdict))
                kinds.append(kind)
                climbs.append(climb)
                tops.append(top)
                clouds.append(cloud)
        days.append({
            "date": (today + datetime.timedelta(days=ahead)).isoformat(),
            "whole": True,
            "verdict": verdicts,
            "why": whys,
            "thermals": kinds,
            "climb_ms": climbs,
            "top_m": tops,
            "cloud": clouds,
        })
    return {
        "source": real.get("source", "open-meteo"),
        "model": real.get("model", "icon-d2"),
        # A run of the map's first morning, as a real map's would be.
        "run": f"{today.isoformat()}T00:00:00Z",
        "computed_at": f"{today.isoformat()}T03:25:00Z",
        "grid": grid,
        "ground_m": ground,
        "days": days,
        "attributions": real.get("attributions", []),
    }


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    behind = ""

    def forward(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else None
        request = urllib.request.Request(self.behind + self.path, data=body, method=self.command)
        for name in PASSED_ON:
            if self.headers.get(name):
                request.add_header(name, self.headers[name])
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                status, headers, answer = response.status, response.headers, response.read()
        except urllib.error.HTTPError as error:
            status, headers, answer = error.code, error.headers, error.read()
        except OSError as error:
            status, headers, answer = 502, {}, str(error).encode()

        handed_back = {name: headers[name] for name in HANDED_BACK if headers.get(name)}
        url = urllib.parse.urlsplit(self.path)
        if self.command == "GET" and url.path == "/v1/forecast" and status == 200:
            query = urllib.parse.parse_qs(url.query)
            try:
                lat, lon = float(query["lat"][0]), float(query["lon"][0])
                now = datetime.datetime.now(datetime.timezone.utc)
                answer = json.dumps(made_up(json.loads(answer), lat, lon, now)).encode()
                # What was changed is not what the tag and the times name.
                handed_back = {"Content-Type": "application/json", "Cache-Control": "no-store"}
            except (KeyError, ValueError):
                pass
        if self.command in ("GET", "HEAD") and url.path == "/v1/thermals":
            try:
                real = json.loads(answer) if status == 200 else None
            except ValueError:
                real = None
            status = 200
            answer = json.dumps(made_up_map(real, self.today())).encode()
            handed_back = {"Content-Type": "application/json", "Cache-Control": "no-store"}
        self.send_response(status)
        for name, value in handed_back.items():
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(answer)

    def today(self):
        """The day by the clock of the server behind, or None if it does
        not say."""
        try:
            with urllib.request.urlopen(self.behind + "/v1/health", timeout=20) as response:
                return datetime.date.fromisoformat(json.loads(response.read())["time"][:10])
        except (OSError, KeyError, TypeError, ValueError):
            return None

    do_GET = do_HEAD = do_POST = do_PUT = do_PATCH = do_DELETE = forward

    def log_message(self, format, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--behind", required=True, help="the development server to stand in front of")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--bind", default="127.0.0.1")
    arguments = parser.parse_args()
    Handler.behind = arguments.behind.rstrip("/")
    server = ThreadingHTTPServer((arguments.bind, arguments.port), Handler)
    print(f"a made-up day on http://{arguments.bind}:{arguments.port}, in front of {Handler.behind}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
