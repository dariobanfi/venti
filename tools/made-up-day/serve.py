#!/usr/bin/env python3
"""A Venti server whose forecast is made up: a strong summer day, today.

For UI tests and their screenshots, and for nothing else. The development
server (`venti-server dev`) answers forecasts from what the weather
models said of four days in late November: rain, cloud and nothing to
climb in. The screens that show a cross-country day or a big day cannot be
seen with it. This program stands in front of a development server and
hands on everything that server answers, except `GET /v1/forecast`: there
it keeps the real answer's place, grid cell, model ground and credit, and
puts made-up hours in place of the model's, for four days, 96 hours, from
the midnight (UTC) of today by the development server's clock: 26 °C
under cumulus at 3500 m with a light south wind, and by day as much heat
from the ground as a summer noon gives. Those are the days its made-up
map has, below: a square of the map and the forecast for its place speak
of the same days. The forecast has every hour of the fourth day, and the
map calls that day not whole: the forecast is one made-up series under
ICON-D2's name, the map's fourth day stands for ICON-EU's. (Until
2026-11-21 the hours began at the real midnight, three days after that
clock's, and the map's "Tomorrow" was a day no forecast had; and there
were three days of both, as the server then had.)

How much heat depends on the place asked for, so that a list of take-offs
shows more than one kind of day: 550, 300 or 70 W/m² at noon, which the
apps' core reads as a big day, a cross-country day and local thermals.

It also answers `GET /v1/thermals` with a made-up thermal map in the
server's wire format, so that a map of the kind of day shows every kind:
a big day in the north-west, a cross-country day beside it, local
thermals in the south-west, weak ones beside those, a marginal band in the
east (by gusts) and a corner there that is not flyable (by rain), as
patches of whole cells.
Four days from today by the development server's clock (the `time` of its
health answer, which is the clock the apps name "Today" by; its recorded
forecasts are of other days), each a step weaker everywhere than the day
before, with climbs and tops that fit each kind. The first two days are
ICON-D2's and the third and the fourth ICON-EU's, as on a real map after
a run of ICON-EU of 00, 06, 12 or 18 UTC. The fourth is marked as not
whole (`"whole": false`), as the last day of a real map mostly is, since
such a run ends on it at 06, 12 or 18 UTC: the apps then show the note of
a day that was judged without all of its daylight. The grid is a small one of its own,
five rows by nine columns of a tenth of a degree from 47.5 N 11.5 E,
unless the development server's map has a grid of 30 cells or more (the
one recorded has four, too few for six patches); the source, model and
credit are the development server's where it has a map. Every value of
the map is made up, deterministically: the same on every start.

And it answers `GET /v1/thermals/hours` with the hours of each day of
that map, a layer at a time as a server does (`date`, `layer`, and for the
wind a `height`), so that the line of hours and the layers of wind, gusts,
cloud and rain can be seen with something in them: eleven hours a day,
from 06 to 16 UTC, in which the thermals come up through the morning and
fade in the late afternoon; what speaks against a square does so from 10
to 15 UTC, rain in the corner that is not flyable and gusts of 32 km/h and
more in the marginal band; a south wind at the ground that strengthens and
veers with height, and has no value at a height under a square's ground
(the made-up ground lies between 700 and 1300 m, so 1000 m is under some
of it); cloud that thickens from west to east. The fourth day, which is
not whole, has its first hour alone. The map names the heights
(`heights_m`), as a server's does, which is what tells an app that there
are hours to ask for.

None of it is a forecast. The answer says so in the name of its model's
run only by being impossible for the season; whoever shows a screenshot
made with it says that the day is made up.

    serve.py --behind http://127.0.0.1:8080 --port 8081

To look at the made-up map and its hours without a server, or to check
them against the apps' core (`VENTI_MADE_UP_DAY=<folder> cargo test -p
venti-core made_up -- --ignored`):

    serve.py --write <folder>
"""

import argparse
import datetime
import json
import pathlib
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


def made_up(answer, lat, lon, now, today=None):
    """The real answer with made-up hours in place of the model's, from
    the midnight of `today`, or of `now` without one."""
    midnight = now.replace(hour=0, minute=0, second=0, microsecond=0)
    if today is not None:
        midnight = datetime.datetime.combine(today, datetime.time(), tzinfo=datetime.timezone.utc)
    # By the thousandths of the place's degrees: neighbours differ.
    heat = HEAT_W_M2[(round(lat * 1000) + round(lon * 1000)) % len(HEAT_W_M2)]
    stamp = lambda time: time.strftime("%Y-%m-%dT%H:%M:%SZ")
    answer["fetched_at"] = stamp(now)
    # The wind around the place is the real answer's, of another season and
    # other days than the made-up hours: without it, the day says nothing of
    # a neighbourhood, which is what a server without a thermal map says.
    answer.pop("nearby", None)
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
            # Four days, as many as the made-up map has.
            one["hours"] = [
                hour(midnight + datetime.timedelta(hours=ahead), heat, ground)
                for ahead in range(96)
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
    computed = real.get("computed_at") or "2026-11-21T09:30:00Z"
    today = today or datetime.date.fromisoformat(computed[:10])
    days = []
    for ahead in range(4):
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
        # The first two days are ICON-D2's, of the map's run. The third and
        # the fourth are ICON-EU's, of its run of the same hour: a run of
        # midnight is one of ICON-EU's main runs, which reach 78 hours, to
        # 06 UTC of the fourth day. So the fourth day is not whole, as the
        # last day of a real map mostly is.
        model = "icon-eu" if ahead >= 2 else "icon-d2"
        run = datetime.datetime.combine(today, datetime.time())
        days.append({
            "date": (today + datetime.timedelta(days=ahead)).isoformat(),
            "model": model,
            "run": run.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "whole": ahead < 3,
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
        # The heights its hours have the wind at: with them an app knows
        # that there are hours to ask for.
        "heights_m": HEIGHTS_M,
        "days": days,
        "attributions": real.get("attributions", []),
    }


# The heights above the sea the made-up map names for the wind of its
# hours, as a server's map does, and the layers its hours come in.
HEIGHTS_M = [1000, 1500, 2000, 2500, 3000, 4000]
LAYERS = ("thermals", "wind", "cloud", "rain")
# The hours (UTC) a made-up day has, and those in which what speaks
# against a square does so.
DAY_HOURS = range(6, 17)
BAD_HOURS = range(10, 16)


class Refused(Exception):
    """A request for hours the made-up map refuses, as a server would."""

    def __init__(self, status, code, message):
        super().__init__(message)
        self.status, self.code = status, code


def made_up_hours(made, date, layer="thermals", height=None):
    """One layer of the hours of one day of a made-up map (`made_up_map`),
    in the wire format of `GET /v1/thermals/hours`."""
    if layer not in LAYERS:
        raise Refused(400, "invalid_request", "layer is one of " + ", ".join(LAYERS))
    if height is not None and layer != "wind":
        raise Refused(400, "invalid_request", "height goes with layer=wind")
    if height is not None and height not in HEIGHTS_M:
        raise Refused(400, "invalid_request",
                      "height is one of " + ", ".join(str(one) for one in HEIGHTS_M))
    day = next((one for one in made["days"] if one["date"] == date), None)
    if day is None:
        raise Refused(404, "not_found", "the thermal map has no hours for " + date + "; it has "
                      + ", ".join(one["date"] for one in made["days"]))
    grid, ground = made["grid"], made["ground_m"]
    rows, columns = grid["rows"], grid["columns"]
    cells = [(row, column) for row in range(rows) for column in range(columns)]
    # A day that is not whole ends with its model's run, early in it.
    clock = list(DAY_HOURS) if day["whole"] else list(DAY_HOURS)[:1]
    hours = []
    for at in clock:
        bad = at in BAD_HOURS
        # Thermals come up through the morning and fade late in the day.
        weaker = 2 if at <= 7 or at >= 16 else 1 if at <= 9 or at == 15 else 0
        lists = {name: [] for name in (
            "verdict", "why", "thermals", "climb_ms", "top_m", "cloud",
            "wind_kmh", "wind_from_deg", "gust_kmh", "cloud_cover_pct", "precipitation_mm")}
        for cell, (row, column) in enumerate(cells):
            of_day = day["verdict"][cell]
            verdict = of_day if bad else "flyable"
            kind = None
            if verdict != "not_flyable":
                kind = day["thermals"][cell] or "weak"
                for _ in range(weaker):
                    kind = WEAKER[kind]
            climb, top, cloud = KINDS[kind] if kind else (None, None, False)
            lists["verdict"].append(verdict)
            lists["why"].append({"not_flyable": "rain", "marginal": "gusts"}.get(verdict))
            lists["thermals"].append(kind)
            lists["climb_ms"].append(climb)
            lists["top_m"].append(top)
            lists["cloud"].append(cloud)

            across = column / max(columns - 1, 1)
            if height is None:
                # A south wind at the ground, lighter in the west, with
                # gusts that pass 30 km/h where they speak against the hour.
                speed = 3 + (row * 3 + column * 5 + at) % 9 + (4 if across >= 0.35 else 0)
                gust = speed + 5 + (row + at) % 5
                if verdict == "marginal":
                    gust = 32 + (row + column) % 7
                if verdict == "not_flyable":
                    speed, gust = 22 + (row + column) % 5, 41 + (row + column) % 4
                lists["wind_kmh"].append(speed)
                lists["wind_from_deg"].append((180 + 10 * ((column + at) % 7 - 3)) % 360)
                lists["gust_kmh"].append(gust)
            elif ground[cell] is not None and ground[cell] > height:
                # Under the ground there is no wind.
                lists["wind_kmh"].append(None)
                lists["wind_from_deg"].append(None)
            else:
                # Stronger and further round to the west with height.
                speed = 4 + height // 250 + (row * 2 + column + at) % 8
                lists["wind_kmh"].append(speed + (6 if across >= 0.35 else 0))
                lists["wind_from_deg"].append(
                    (200 + height // 100 + 10 * ((row + at) % 5)) // 10 * 10 % 360)

            cover = 10 + 10 * ((row + column + at) % 4)
            cover += 60 if across >= 0.7 else 30 if across >= 0.35 else 0
            rain = 0.0
            if verdict == "not_flyable":
                cover, rain = 100, (0.4, 1.6, 5.2)[(row + column + at) % 3]
            elif verdict == "marginal" and (row + at) % 4 == 0:
                rain = 0.1
            lists["cloud_cover_pct"].append(min(cover, 100))
            lists["precipitation_mm"].append(rain)
        keep = {
            "thermals": ("verdict", "why", "thermals", "climb_ms", "top_m", "cloud"),
            "wind": ("wind_kmh", "wind_from_deg") + (("gust_kmh",) if height is None else ()),
            "cloud": ("cloud_cover_pct",),
            "rain": ("precipitation_mm",),
        }[layer]
        hours.append({"time": f"{date}T{at:02d}:00:00Z", **{name: lists[name] for name in keep}})
    answer = {
        "source": made["source"],
        "date": date,
        "layer": layer,
        "heights_m": HEIGHTS_M,
        "model": day["model"],
        "run": day["run"],
        "whole": day["whole"],
        "computed_at": made["computed_at"],
        "grid": grid,
        "ground_m": ground,
        "hours": hours,
        "attributions": made["attributions"],
    }
    if height is not None:
        answer["height_m"] = height
    return answer


def hours_asked(made, query):
    """The answer to a query for hours, as (status, body)."""
    try:
        date = (query.get("date") or [""])[0]
        try:
            datetime.date.fromisoformat(date)
        except ValueError:
            raise Refused(400, "invalid_day", "give the day as date=YYYY-MM-DD") from None
        height = (query.get("height") or [None])[0]
        if height is not None:
            try:
                height = int(height)
            except ValueError:
                raise Refused(400, "invalid_request", "height is a number of metres") from None
        layer = (query.get("layer") or ["thermals"])[0]
        return 200, made_up_hours(made, date, layer, height)
    except Refused as refused:
        return refused.status, {"error": {"code": refused.code, "message": str(refused)}}


def write(folder):
    """Writes the made-up map and every layer of each of its days to a
    folder, as files named after what was asked for."""
    folder = pathlib.Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    made = made_up_map(None)
    (folder / "thermals.json").write_text(json.dumps(made))
    for day in made["days"]:
        for layer in LAYERS:
            for height in [None] + (HEIGHTS_M if layer == "wind" else []):
                name = f"hours_{day['date']}_{layer}" + (f"_{height}" if height else "")
                answer = made_up_hours(made, day["date"], layer, height)
                (folder / f"{name}.json").write_text(json.dumps(answer))


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
                answer = json.dumps(made_up(json.loads(answer), lat, lon, now, self.today())).encode()
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
        if self.command in ("GET", "HEAD") and url.path == "/v1/thermals/hours":
            # The hours of the made-up map's days, whatever the server
            # behind said of its own.
            status, made = hours_asked(
                made_up_map(self.real_map(), self.today()), urllib.parse.parse_qs(url.query))
            answer = json.dumps(made).encode()
            handed_back = {"Content-Type": "application/json", "Cache-Control": "no-store"}
        self.send_response(status)
        for name, value in handed_back.items():
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(answer)

    def real_map(self):
        """The thermal map of the server behind, or None if it has none."""
        try:
            with urllib.request.urlopen(self.behind + "/v1/thermals", timeout=20) as response:
                return json.loads(response.read())
        except (OSError, ValueError):
            return None

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
    parser.add_argument("--behind", help="the development server to stand in front of")
    parser.add_argument("--port", type=int)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--write", metavar="FOLDER",
                        help="write the made-up map and its hours to a folder, and serve nothing")
    arguments = parser.parse_args()
    if arguments.write:
        write(arguments.write)
        return
    if not arguments.behind or arguments.port is None:
        parser.error("--behind and --port are needed to serve")
    Handler.behind = arguments.behind.rstrip("/")
    server = ThreadingHTTPServer((arguments.bind, arguments.port), Handler)
    print(f"a made-up day on http://{arguments.bind}:{arguments.port}, in front of {Handler.behind}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
