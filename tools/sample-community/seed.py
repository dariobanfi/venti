#!/usr/bin/env python3
"""Puts the seven made-up sample pilots into a running Venti.

Their accounts (by invite codes where the server asks for them), countries,
pictures, public flights, places in the development mode's six public
groups (made by one of them where the server has no group of that name) and their
messages. What is there already is left as it is, so it can run again.

The pilots, pictures and flights are what
`cargo run -p venti-server --example sample_community` writes, which this
script runs unless --data names a folder it wrote. Python's standard
library only.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

REPOSITORY = pathlib.Path(__file__).resolve().parents[2]
# The development mode's password for its made-up pilots.
DEV_PASSWORD = "demo-pilot-password"
DEVICE = "sample-community script"


class Refused(Exception):
    def __init__(self, status, code, message):
        super().__init__(f"{status} {code}: {message}")
        self.status, self.code = status, code


class Server:
    def __init__(self, address):
        self.address = address.rstrip("/")

    def ask(self, method, path, token=None, body=None, content_type=None):
        """The answer's status and JSON (None for none)."""
        headers = {"Accept": "application/json", "User-Agent": "venti-sample-community"}
        data = None
        if isinstance(body, (bytes, bytearray)):
            data = bytes(body)
            headers["Content-Type"] = content_type
        elif body is not None:
            data = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        if token:
            headers["Authorization"] = f"Bearer {token}"
        request = urllib.request.Request(
            self.address + path, data=data, method=method, headers=headers
        )
        try:
            with urllib.request.urlopen(request, timeout=120) as answer:
                text = answer.read()
                return answer.status, json.loads(text) if text else None
        except urllib.error.HTTPError as error:
            text = error.read()
            try:
                problem = json.loads(text)["error"]
            except (ValueError, KeyError, TypeError):
                problem = {}
            raise Refused(
                error.code,
                problem.get("code", "?"),
                problem.get("message", text[:200].decode(errors="replace")),
            ) from None


def written_data(folder):
    if folder:
        return pathlib.Path(folder)
    folder = pathlib.Path(tempfile.mkdtemp(prefix="venti-sample-community-"))
    print("Writing the sample pilots' files with cargo …", flush=True)
    subprocess.run(
        ["cargo", "run", "--quiet", "-p", "venti-server", "--example", "sample_community", "--", str(folder)],
        cwd=REPOSITORY,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    return folder


def sign_in(server, pilot, password, invites):
    """A session's token and identifier for the pilot, making the account
    if there is none."""
    handle = pilot["handle"]
    try:
        _, answer = server.ask(
            "POST", "/v1/sessions", body={"handle": handle, "password": password, "device": DEVICE}
        )
        print(f"  {handle}: signed in")
        return answer["token"], answer["session"]["id"]
    except Refused as refused:
        if refused.code != "invalid_credentials":
            raise
    body = {
        "handle": handle,
        "display_name": pilot["display_name"],
        "password": password,
        "device": DEVICE,
    }
    if invites:
        body["invite"] = invites[0]
    try:
        _, answer = server.ask("POST", "/v1/accounts", body=body)
    except Refused as refused:
        if refused.code == "invalid_invite":
            sys.exit(
                f"{handle}: the server wants an invite code for each new account "
                "(--invite CODE, once per code); one given was used or mistyped"
            )
        if refused.code == "handle_taken":
            sys.exit(f"{handle}: an account of that handle exists with another password")
        raise
    if invites:
        invites.pop(0)
    print(f"  {handle}: account made")
    return answer["token"], answer["session"]["id"]


def seed_pilot(server, folder, pilot, token):
    handle = pilot["handle"]
    _, answer = server.ask("GET", "/v1/account", token)
    account = answer["account"]
    if account.get("country") != pilot["country"]:
        server.ask("PATCH", "/v1/account", token, {"country": pilot["country"]})
        print(f"  {handle}: country {pilot['country']}")
    if not account.get("picture"):
        picture = (folder / pilot["picture"]).read_bytes()
        server.ask("PUT", "/v1/account/picture", token, picture, "image/png")
        print(f"  {handle}: picture sent")
    for name in pilot["flights"]:
        status, answer = server.ask(
            "POST", "/v1/flights", token, (folder / name).read_bytes(), "application/octet-stream"
        )
        flight = answer["flight"]
        if status == 201:
            print(f"  {handle}: flight {name} uploaded, {flight['visibility']}")
        if flight["visibility"] != "public":
            server.ask("PATCH", f"/v1/flights/{flight['id']}", token, {"visibility": "public"})
            print(f"  {handle}: flight {name} made public")


def find_group(server, token, name):
    query = urllib.parse.urlencode({"q": name, "limit": 50})
    _, answer = server.ask("GET", f"/v1/groups/find?{query}", token)
    for found in answer["groups"]:
        if found["group"]["name"] == name:
            return found["group"]["id"]
    return None


def seed_groups(server, manifest, tokens, suffix):
    by_name = {group["name"]: group for group in manifest["groups"]}
    for name, group in by_name.items():
        members = [p for p in manifest["pilots"] if name in p["groups"]]
        if not members:
            continue
        shown = name + suffix
        first = tokens[members[0]["handle"]]
        group_id = find_group(server, first, shown)
        if group_id is None:
            body = {
                "name": shown,
                "public": True,
                "description": group["description"],
                "home": group["home"],
            }
            _, answer = server.ask("POST", "/v1/groups", first, body)
            group_id = answer["group"]["id"]
            print(f"  {shown}: made by {members[0]['handle']}")
        for pilot in members:
            status, _ = server.ask("POST", f"/v1/groups/{group_id}/members", tokens[pilot["handle"]])
            if status == 201:
                print(f"  {shown}: {pilot['handle']} came in")
        # The messages, the oldest first, each once.
        _, answer = server.ask("GET", f"/v1/groups/{group_id}/messages?limit=100", first)
        there = {(m["author"]["handle"], m.get("text")) for m in answer["messages"]}
        messages = [
            (message["minutes_ago"], pilot["handle"], message["text"])
            for pilot in members
            for message in pilot["messages"]
            if message["group"] == name
        ]
        for _, handle, text in sorted(messages, reverse=True):
            if (handle, text) in there:
                continue
            server.ask("POST", f"/v1/groups/{group_id}/messages", tokens[handle], {"text": text})
            print(f"  {shown}: {handle} wrote")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("server", help="the server's address, such as http://127.0.0.1:8088")
    parser.add_argument(
        "--invite",
        action="append",
        default=[],
        help="an invite code for a new account; once per code, as many as accounts are missing",
    )
    parser.add_argument(
        "--password",
        default=DEV_PASSWORD,
        help="the sample pilots' password (default: the development mode's, for a stack "
        "on this computer only)",
    )
    parser.add_argument("--data", help="a folder the example wrote already")
    parser.add_argument(
        "--suffix",
        default="",
        help="appended to every handle and group name: to try the script again on a "
        "server that has the pilots already",
    )
    arguments = parser.parse_args()

    server = Server(arguments.server)
    folder = written_data(arguments.data)
    manifest = json.loads((folder / "sample-community.json").read_text())
    if arguments.suffix:
        for pilot in manifest["pilots"]:
            pilot["handle"] += arguments.suffix
    invites = list(arguments.invite)
    tokens, sessions = {}, {}
    try:
        print("Pilots:")
        for pilot in manifest["pilots"]:
            token, sessions[token] = sign_in(server, pilot, arguments.password, invites)
            tokens[pilot["handle"]] = token
            seed_pilot(server, folder, pilot, token)
        print("Groups:")
        seed_groups(server, manifest, tokens, arguments.suffix)
    except Refused as refused:
        sys.exit(f"The server refused: {refused}")
    except urllib.error.URLError as error:
        sys.exit(f"{arguments.server} could not be reached: {error.reason}")
    finally:
        # Sign the script's sessions out again.
        for token, session in sessions.items():
            try:
                server.ask("DELETE", f"/v1/sessions/{session}", token)
            except (Refused, urllib.error.URLError):
                pass
    print("Done.")


if __name__ == "__main__":
    main()
