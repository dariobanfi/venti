//! The command line of `venti-server`.

use std::net::SocketAddr;
use std::path::PathBuf;

use venti_domain::SourceId;

use crate::accounts::invite;
use crate::ingest::oneshot;
use crate::moderation::{self, Action};

/// What `venti-server --help` prints.
pub const USAGE: &str = "\
usage: venti-server <command>

  serve [--config FILE]
      Run the server: collect the configured sources on their schedule,
      store them in PostgreSQL and serve the API. Settings come from FILE
      and from VENTI_* environment variables; see config.example.toml.

  dev [--listen ADDRESS [--open-to-the-network]] [--maps FOLDER]
      [--online-imagery]
      Serve the same API over recorded data and a demo account. No
      network, no database. Listens on 127.0.0.1:8080 unless told
      otherwise. This mode has a demo account whose password is public,
      takes accounts from anyone and has no limits on attempts, so it
      listens on an address other machines can reach only when
      --open-to-the-network says so as well: for a phone on your own
      network, or inside a container. With --maps it hands out the map
      regions in FOLDER as serve hands out those of its maps_dir, and
      has read them before it listens; without, it hands out none. The
      region the apps bundle is in apps/ios/Venti/Resources. Map
      imagery (/v1/imagery) comes from the few tiles recorded from its
      providers, and every other tile is empty; with --online-imagery
      this one route asks the providers over the network and keeps their
      tiles in venti-dev-imagery under the system's temporary folder.

  invite [--config FILE] [--count N] [--expires-in-days N]
      Make invite codes and print them, one per line. Each creates one
      account on a server whose registration is by invite. Uses the
      database from FILE or the environment, as serve does.

  reports [--all] [--config FILE]
      List what pilots reported, oldest first: the open reports, or with
      --all the closed ones too. Each names what is reported, the reason,
      the reporter's words and who reported. Changes nothing.

  close-report <ID> --note TEXT [--config FILE]
      Close a report with a note that says what was done, or why nothing
      was. Changes nothing about what was reported.

  take-down <FLIGHT> [--config FILE]
      Make a flight private, whatever its pilot had set, and keep it so:
      its pilot cannot share it again until it is released. FLIGHT is the
      flight's address or a link to its page. Deletes nothing.

  release <FLIGHT> [--config FILE]
      Let a flight that was taken down go. It stays private until its
      pilot shares it.

  remove-live-track <TRACK> [--config FILE]
      Remove a live track with its positions and take its identifier out
      of use. TRACK is the track's identifier or a link to its page.

  suspend-publishing <HANDLE> [--config FILE]
      Take from an account the ability to publish: its public flights
      become unlisted, it can make none public, and its live tracks are
      for those with their link. The account is not deleted and works
      otherwise.

  resume-publishing <HANDLE> [--config FILE]
      Give it back. Makes nothing public again.

  group <GROUP> [--config FILE]
      Show one group as its members see it: its name, who keeps it, who
      is in it, and its plans. GROUP is the group's identifier, as
      reports prints it. Changes nothing.

  rename-group <GROUP> --name TEXT [--config FILE]
      Give a group another name. Tells nobody; its keeper can rename it
      again.

  remove-plan <PLAN> [--config FILE]
      Remove one plan of a group, with who was in on it. PLAN is the
      plan's identifier, as reports and group print it. Tells nobody, and
      cannot be undone.

  remove-group <GROUP> [--config FILE]
      Remove a group with its members' places in it and all its plans.
      Touches no account, tells nobody, and cannot be undone.

  site <SITE> [--config FILE]
      Show one site that pilots made: what it says, who made it, where it
      stands, and every revision of its history. SITE is the site's
      identifier (venti: and 26 characters) or a link that ends in it.
      Changes nothing.

  revert-site <SITE> --revision N [--comment TEXT] [--config FILE]
      Put a site that pilots made back to what its revision N said, as a
      new revision of the operator's. Brings back a site that was removed
      or taken down. The comment is kept in the site's history, which
      everyone can read.

  remove-site <SITE> [--comment TEXT] [--config FILE]
      Take a site that pilots made down: it is gone to everyone, and its
      identifier is never used again. Deletes nothing; revert-site brings
      it back.

  grant-sites <HANDLE> [--config FILE]
      Name an account as one of Venti's partners for flying sites: it
      adds sites to the partners' layer, which every app shows, and
      corrects sites of both layers. Nothing else about the account
      changes.

  withdraw-sites <HANDLE> [--config FILE]
      Take that back. Every site and revision the account wrote stays.

  site-partners [--config FILE]
      List the accounts that are partners for flying sites, by handle,
      with since when. Changes nothing.

  ingest <region.toml> [--source ID] [--archive DIR] [--json]
      Run a region's sources once against the live providers and print a
      health line for each. Stores nothing. Fails when a source delivers
      nothing or reports problems.

  derive
      Read a flight file from standard input and print what the server
      works out from it. The server runs this itself for every upload, so
      that a file it cannot cope with costs a child process and not the
      server.

  --help, --version";

/// What the program was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the server.
    Serve {
        /// The configuration file, if one was named.
        config: Option<PathBuf>,
    },
    /// Serve recorded data.
    Dev {
        /// The address to listen on.
        listen: SocketAddr,
        /// The folder of the map regions to hand out, if one was named.
        maps: Option<PathBuf>,
        /// Whether map imagery is asked of its providers over the network.
        online_imagery: bool,
    },
    /// Run a region's sources once.
    Ingest(oneshot::Args),
    /// Make invite codes.
    Invite(invite::Args),
    /// Read the reports, or act on a flight, a live track, an account, a
    /// group, a plan or a site of Venti's own, or say who Venti's
    /// partners for flying sites are, as whoever runs the server.
    Moderate(moderation::Args),
    /// Analyse the flight file on standard input.
    Derive,
    /// Print the usage.
    Help,
    /// Print the version.
    Version,
}

/// Reads the arguments that follow the program's name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or("no command given")?;
    match command.as_str() {
        "serve" => parse_serve(args),
        "dev" => parse_dev(args),
        "ingest" => parse_ingest(args),
        "invite" => parse_invite(args),
        "reports" | "close-report" | "take-down" | "release" | "remove-live-track"
        | "suspend-publishing" | "resume-publishing" | "group" | "rename-group" | "remove-plan"
        | "remove-group" | "site" | "revert-site" | "remove-site" | "grant-sites"
        | "withdraw-sites" | "site-partners" => parse_moderation(&command, args),
        "derive" => match args.next() {
            None => Ok(Command::Derive),
            Some(other) => Err(format!("derive does not take {other:?}")),
        },
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" => Ok(Command::Version),
        other => Err(format!("unknown command {other:?}")),
    }
}

/// The value that must follow an option.
fn value(option: &str, args: &mut impl Iterator<Item = String>) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{option} needs a value"))
}

fn parse_serve(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut config = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(PathBuf::from(value("--config", &mut args)?)),
            other => return Err(format!("serve does not take {other:?}")),
        }
    }
    Ok(Command::Serve { config })
}

fn parse_dev(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut listen = SocketAddr::from(([127, 0, 0, 1], 8080));
    let mut open_to_the_network = false;
    let mut maps = None;
    let mut online_imagery = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open-to-the-network" => open_to_the_network = true,
            "--online-imagery" => online_imagery = true,
            "--maps" => maps = Some(PathBuf::from(value("--maps", &mut args)?)),
            "--listen" => {
                let address = value("--listen", &mut args)?;
                listen = address
                    .parse()
                    .map_err(|_| format!("{address:?} is not an address such as 127.0.0.1:8080"))?;
            }
            other => return Err(format!("dev does not take {other:?}")),
        }
    }
    // The development mode is no server: its account's password and token
    // are written in the documentation, anyone may add accounts, and no
    // attempt is counted. On this machine's own address that harms nobody.
    // On any other it is an open door, so it takes a second word.
    if !listen.ip().is_loopback() && !open_to_the_network {
        return Err(format!(
            "dev would listen on {listen}, which other machines can reach, with a demo account \
             whose password is public and no limits; add --open-to-the-network if that is \
             meant (a phone on your own network, a container), or run `serve` for a real server"
        ));
    }
    Ok(Command::Dev {
        listen,
        maps,
        online_imagery,
    })
}

fn parse_ingest(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut region = None;
    let mut source = None;
    let mut archive = None;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" => {
                let id = value("--source", &mut args)?;
                source = Some(SourceId::new(id).map_err(|e| e.to_string())?);
            }
            "--archive" => archive = Some(PathBuf::from(value("--archive", &mut args)?)),
            "--json" => json = true,
            _ if arg.starts_with("--") => return Err(format!("ingest does not take {arg:?}")),
            _ if region.is_none() => region = Some(PathBuf::from(arg)),
            _ => return Err("ingest takes one region file".to_owned()),
        }
    }
    Ok(Command::Ingest(oneshot::Args {
        region: region.ok_or("ingest needs a region file")?,
        source,
        archive,
        json,
    }))
}

fn parse_invite(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut parsed = invite::Args {
        config: None,
        count: 1,
        expires_in_days: None,
    };
    let number = |option: &str, text: String, most: u32| {
        text.parse::<u32>()
            .ok()
            .filter(|n| (1..=most).contains(n))
            .ok_or_else(|| format!("{option} takes a number from 1 to {most}, not {text:?}"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => parsed.config = Some(PathBuf::from(value("--config", &mut args)?)),
            "--count" => {
                parsed.count = number("--count", value("--count", &mut args)?, invite::MAX_COUNT)?;
            }
            "--expires-in-days" => {
                let days = value("--expires-in-days", &mut args)?;
                parsed.expires_in_days = Some(number("--expires-in-days", days, 3650)?);
            }
            other => return Err(format!("invite does not take {other:?}")),
        }
    }
    Ok(Command::Invite(parsed))
}

/// Reads one of the operator's subcommands: the one thing each is pointed
/// at, `--config`, and what only some of them take (`--all`, `--note`,
/// `--name`, `--revision`, `--comment`).
fn parse_moderation(
    command: &str,
    mut args: impl Iterator<Item = String>,
) -> Result<Command, String> {
    let mut config = None;
    let mut target = None;
    let mut all = false;
    let mut note = None;
    let mut name = None;
    let mut revision = None;
    let mut comment = None;
    let about_a_site = matches!(command, "revert-site" | "remove-site");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(PathBuf::from(value("--config", &mut args)?)),
            "--all" if command == "reports" => all = true,
            "--note" if command == "close-report" => note = Some(value("--note", &mut args)?),
            "--name" if command == "rename-group" => name = Some(value("--name", &mut args)?),
            "--revision" if command == "revert-site" => {
                let text = value("--revision", &mut args)?;
                let number = text.parse::<u32>().ok().filter(|number| *number >= 1);
                revision = Some(number.ok_or_else(|| {
                    format!("--revision takes the number of a revision, from 1 on, not {text:?}")
                })?);
            }
            "--comment" if about_a_site => comment = Some(value("--comment", &mut args)?),
            _ if arg.starts_with("--") => return Err(format!("{command} does not take {arg:?}")),
            _ if matches!(command, "reports" | "site-partners") => {
                return Err(format!("{command} does not take {arg:?}"));
            }
            _ if target.is_none() => target = Some(arg),
            _ => {
                return Err(format!(
                    "{command} takes one thing to act on, not {arg:?} too"
                ));
            }
        }
    }
    let mut target = |what: &str| target.take().ok_or(format!("{command} needs {what}"));
    let action = match command {
        "reports" => Action::Reports { all },
        "close-report" => Action::CloseReport {
            id: target("the identifier of a report")?,
            note: note.ok_or("close-report needs --note with what was done, or why nothing was")?,
        },
        "take-down" => Action::TakeDown {
            flight: target("a flight's address or link")?,
        },
        "release" => Action::Release {
            flight: target("a flight's address or link")?,
        },
        "remove-live-track" => Action::RemoveLiveTrack {
            track: target("a live track's identifier or link")?,
        },
        "suspend-publishing" => Action::SuspendPublishing {
            handle: target("an account's handle")?,
        },
        "resume-publishing" => Action::ResumePublishing {
            handle: target("an account's handle")?,
        },
        "group" => Action::Group {
            group: target("a group's identifier")?,
        },
        "rename-group" => Action::RenameGroup {
            group: target("a group's identifier")?,
            name: name.ok_or("rename-group needs --name with the name the group is to have")?,
        },
        "remove-plan" => Action::RemovePlan {
            plan: target("a plan's identifier")?,
        },
        "remove-group" => Action::RemoveGroup {
            group: target("a group's identifier")?,
        },
        "site" => Action::Site {
            site: target("a site's identifier")?,
        },
        "revert-site" => Action::RevertSite {
            site: target("a site's identifier")?,
            revision: revision
                .ok_or("revert-site needs --revision with the number of the revision to restore")?,
            comment,
        },
        "remove-site" => Action::RemoveSite {
            site: target("a site's identifier")?,
            comment,
        },
        "grant-sites" => Action::GrantSites {
            handle: target("an account's handle")?,
        },
        "withdraw-sites" => Action::WithdrawSites {
            handle: target("an account's handle")?,
        },
        "site-partners" => Action::SitePartners,
        other => return Err(format!("unknown command {other:?}")),
    };
    Ok(Command::Moderate(moderation::Args { config, action }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line(line: &str) -> Result<Command, String> {
        parse(line.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn serve_takes_an_optional_configuration_file() {
        assert_eq!(parse_line("serve"), Ok(Command::Serve { config: None }));
        assert_eq!(
            parse_line("serve --config var/venti-server.toml"),
            Ok(Command::Serve {
                config: Some(PathBuf::from("var/venti-server.toml"))
            })
        );
        assert!(parse_line("serve --config").is_err());
        assert!(parse_line("serve --port 80").is_err());
    }

    #[test]
    fn dev_listens_on_the_loopback_address_unless_told_otherwise() {
        assert_eq!(
            parse_line("dev"),
            Ok(Command::Dev {
                listen: "127.0.0.1:8080".parse().unwrap(),
                maps: None,
                online_imagery: false,
            })
        );
        assert_eq!(
            parse_line("dev --listen [::1]:9000"),
            Ok(Command::Dev {
                listen: "[::1]:9000".parse().unwrap(),
                maps: None,
                online_imagery: false,
            })
        );
        assert!(parse_line("dev --listen localhost").is_err());
    }

    #[test]
    fn dev_hands_out_the_map_regions_of_a_folder_when_one_is_named() {
        assert_eq!(
            parse_line("dev --maps apps/ios/Venti/Resources"),
            Ok(Command::Dev {
                listen: "127.0.0.1:8080".parse().unwrap(),
                maps: Some(PathBuf::from("apps/ios/Venti/Resources")),
                online_imagery: false,
            })
        );
        // Before or after where to listen.
        assert_eq!(
            parse_line("dev --maps ../maps --listen 127.0.0.1:9000"),
            Ok(Command::Dev {
                listen: "127.0.0.1:9000".parse().unwrap(),
                maps: Some(PathBuf::from("../maps")),
                online_imagery: false,
            })
        );
        assert!(parse_line("dev --maps").is_err());
        assert!(USAGE.contains("[--maps FOLDER]"));
        // Only the development mode takes the option: a server is told
        // in its configuration.
        assert!(parse_line("serve --maps var/maps").is_err());
    }

    #[test]
    fn dev_asks_providers_of_imagery_only_when_told() {
        assert_eq!(
            parse_line("dev --online-imagery --maps ../maps"),
            Ok(Command::Dev {
                listen: "127.0.0.1:8080".parse().unwrap(),
                maps: Some(PathBuf::from("../maps")),
                online_imagery: true,
            })
        );
        assert!(USAGE.contains("[--online-imagery]"));
        assert!(parse_line("serve --online-imagery").is_err());
    }

    #[test]
    fn dev_listens_where_others_can_reach_it_only_when_told_twice() {
        // Every address of the machine, a private one, a public one: each
        // is reachable from elsewhere, and none is taken on one word.
        for address in [
            "0.0.0.0:8080",
            "192.168.1.20:8080",
            "203.0.113.9:80",
            "[::]:8080",
        ] {
            let refused = parse_line(&format!("dev --listen {address}")).unwrap_err();
            assert!(refused.contains("--open-to-the-network"), "{refused}");
            assert!(refused.contains("password is public"), "{refused}");
            assert_eq!(
                parse_line(&format!("dev --listen {address} --open-to-the-network")),
                Ok(Command::Dev {
                    listen: address.parse().unwrap(),
                    maps: None,
                    online_imagery: false,
                })
            );
        }
        // The second word in front works as well, and alone it changes
        // nothing.
        assert!(parse_line("dev --open-to-the-network --listen 0.0.0.0:8080").is_ok());
        assert_eq!(
            parse_line("dev --open-to-the-network"),
            Ok(Command::Dev {
                listen: "127.0.0.1:8080".parse().unwrap(),
                maps: None,
                online_imagery: false,
            })
        );
    }

    #[test]
    fn ingest_keeps_the_options_of_the_tool_it_replaces() {
        let command = parse_line(
            "ingest regions/alps-de-at/region.toml --source dwd-10min --archive var/raw --json",
        );
        assert_eq!(
            command,
            Ok(Command::Ingest(oneshot::Args {
                region: PathBuf::from("regions/alps-de-at/region.toml"),
                source: Some(SourceId::new("dwd-10min").unwrap()),
                archive: Some(PathBuf::from("var/raw")),
                json: true,
            }))
        );
        assert!(parse_line("ingest").is_err());
        assert!(parse_line("ingest a.toml b.toml").is_err());
        assert!(parse_line("ingest a.toml --verbose").is_err());
        assert!(parse_line("ingest a.toml --source Not/An/Id").is_err());
    }

    #[test]
    fn invite_makes_one_code_that_does_not_expire_unless_told_otherwise() {
        assert_eq!(
            parse_line("invite"),
            Ok(Command::Invite(invite::Args {
                config: None,
                count: 1,
                expires_in_days: None,
            }))
        );
        assert_eq!(
            parse_line("invite --count 5 --expires-in-days 14 --config var/venti-server.toml"),
            Ok(Command::Invite(invite::Args {
                config: Some(PathBuf::from("var/venti-server.toml")),
                count: 5,
                expires_in_days: Some(14),
            }))
        );
        assert!(parse_line("invite --count 0").is_err());
        assert!(parse_line("invite --count 101").is_err());
        assert!(parse_line("invite --count many").is_err());
        assert!(parse_line("invite --expires-in-days 0").is_err());
        assert!(parse_line("invite --expires-in-days").is_err());
        assert!(parse_line("invite dario").is_err());
    }

    #[test]
    fn the_operators_commands_each_take_the_one_thing_they_act_on() {
        let moderate = |line: &str| match parse_line(line) {
            Ok(Command::Moderate(args)) => Ok(args),
            Ok(other) => panic!("{line}: {other:?}"),
            Err(error) => Err(error),
        };
        let action = |line: &str| moderate(line).unwrap().action;

        assert_eq!(action("reports"), Action::Reports { all: false });
        assert_eq!(action("reports --all"), Action::Reports { all: true });
        let flight = "5t1eshypdtvrq7rxw39a18vsrg".to_owned();
        assert_eq!(
            action("take-down 5t1eshypdtvrq7rxw39a18vsrg"),
            Action::TakeDown {
                flight: flight.clone()
            }
        );
        // A link is taken as it is; what it leads to is looked at later.
        assert_eq!(
            action("release https://venti.example/f/5t1eshypdtvrq7rxw39a18vsrg"),
            Action::Release {
                flight: format!("https://venti.example/f/{flight}")
            }
        );
        assert_eq!(
            action("remove-live-track 11111111111111111111111111"),
            Action::RemoveLiveTrack {
                track: "1".repeat(26)
            }
        );
        assert_eq!(
            action("suspend-publishing mara"),
            Action::SuspendPublishing {
                handle: "mara".to_owned()
            }
        );
        assert_eq!(
            action("resume-publishing mara"),
            Action::ResumePublishing {
                handle: "mara".to_owned()
            }
        );
        // A note is one argument, whatever stands in it; the configuration
        // file may be named before or after.
        let closing = parse(
            [
                "close-report",
                "--config",
                "var/venti-server.toml",
                "0123456789abcdefghjkmnpqrs",
                "--note",
                "Taken down. Told nobody --all.",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            closing,
            Ok(Command::Moderate(moderation::Args {
                config: Some(PathBuf::from("var/venti-server.toml")),
                action: Action::CloseReport {
                    id: "0123456789abcdefghjkmnpqrs".to_owned(),
                    note: "Taken down. Told nobody --all.".to_owned(),
                },
            }))
        );
        assert_eq!(
            moderate("take-down x --config f.toml").unwrap().config,
            Some(PathBuf::from("f.toml"))
        );
        // What is done about a group and a plan: each by an identifier, as
        // `reports` prints it.
        let group = "44444444444444444444444444".to_owned();
        assert_eq!(
            action("group 44444444444444444444444444"),
            Action::Group {
                group: group.clone()
            }
        );
        assert_eq!(
            action("remove-plan 77777777777777777777777777"),
            Action::RemovePlan {
                plan: "7".repeat(26)
            }
        );
        assert_eq!(
            action("remove-group 44444444444444444444444444"),
            Action::RemoveGroup {
                group: group.clone()
            }
        );
        // A name is one argument, whatever stands in it.
        let renaming = parse(
            [
                "rename-group",
                "44444444444444444444444444",
                "--name",
                "Flying --all weekend",
                "--config",
                "f.toml",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            renaming,
            Ok(Command::Moderate(moderation::Args {
                config: Some(PathBuf::from("f.toml")),
                action: Action::RenameGroup {
                    group,
                    name: "Flying --all weekend".to_owned(),
                },
            }))
        );

        // What is done about a site that pilots made: by its identifier,
        // a revision by its number, and a comment that is one argument.
        let site = "venti:55555555555555555555555555".to_owned();
        assert_eq!(
            action("site venti:55555555555555555555555555"),
            Action::Site { site: site.clone() }
        );
        assert_eq!(
            action("remove-site venti:55555555555555555555555555"),
            Action::RemoveSite {
                site: site.clone(),
                comment: None
            }
        );
        let reverting = parse(
            [
                "revert-site",
                "--revision",
                "2",
                "venti:55555555555555555555555555",
                "--comment",
                "Back to what --all agreed on",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            reverting,
            Ok(Command::Moderate(moderation::Args {
                config: None,
                action: Action::RevertSite {
                    site,
                    revision: 2,
                    comment: Some("Back to what --all agreed on".to_owned()),
                },
            }))
        );

        for mistake in [
            "site",
            "site a b",
            "site a --comment x",
            "site a --revision 1",
            "sites",
            "revert-site",
            "revert-site a",
            "revert-site a --revision",
            "revert-site a --revision 0",
            "revert-site a --revision -1",
            "revert-site a --revision two",
            "revert-site --revision 2",
            "revert-site a --revision 2 --comment",
            "revert-site a --revision 2 --note x",
            "remove-site",
            "remove-site a b",
            "remove-site a --revision 2",
            "remove-site a --all",
            "reports demo",
            "reports --note x",
            "close-report",
            "close-report 0123456789abcdefghjkmnpqrs",
            "close-report --note done",
            "close-report 0123456789abcdefghjkmnpqrs --note",
            "take-down",
            "take-down a b",
            "take-down a --all",
            "release",
            "remove-live-track",
            "suspend-publishing",
            "suspend-publishing mara jonas",
            "resume-publishing --config",
            "suspend mara",
            "delete-account mara",
            "group",
            "group a b",
            "group a --name b",
            "groups",
            "rename-group",
            "rename-group 44444444444444444444444444",
            "rename-group --name x",
            "rename-group 44444444444444444444444444 --name",
            "rename-group 44444444444444444444444444 --note x",
            "remove-plan",
            "remove-plan a b",
            "remove-group",
            "remove-group a --all",
            "remove-member mara",
            "grant-sites",
            "grant-sites mara jonas",
            "withdraw-sites",
            "withdraw-sites mara --all",
            "site-partners mara",
            "site-partners --all",
        ] {
            assert!(parse_line(mistake).is_err(), "{mistake}");
        }
        // Every one of them is in the usage.
        for command in [
            "reports [--all]",
            "close-report <ID> --note TEXT",
            "take-down <FLIGHT>",
            "release <FLIGHT>",
            "remove-live-track <TRACK>",
            "suspend-publishing <HANDLE>",
            "resume-publishing <HANDLE>",
            "group <GROUP>",
            "rename-group <GROUP> --name TEXT",
            "remove-plan <PLAN>",
            "remove-group <GROUP>",
            "site <SITE>",
            "revert-site <SITE> --revision N [--comment TEXT]",
            "remove-site <SITE> [--comment TEXT]",
            "grant-sites <HANDLE>",
            "withdraw-sites <HANDLE>",
            "site-partners [--config FILE]",
        ] {
            assert!(USAGE.contains(command), "{command}");
        }
        assert_eq!(
            action("grant-sites mara"),
            Action::GrantSites {
                handle: "mara".to_owned()
            }
        );
        assert_eq!(
            action("withdraw-sites @mara"),
            Action::WithdrawSites {
                handle: "@mara".to_owned()
            }
        );
        assert_eq!(action("site-partners"), Action::SitePartners);
    }

    #[test]
    fn derive_takes_nothing_but_its_input() {
        assert_eq!(parse_line("derive"), Ok(Command::Derive));
        assert!(parse_line("derive flight.igc").is_err());
    }

    #[test]
    fn help_version_and_mistakes() {
        assert_eq!(parse_line("--help"), Ok(Command::Help));
        assert_eq!(parse_line("--version"), Ok(Command::Version));
        assert!(parse_line("").is_err());
        assert!(parse_line("migrate").is_err());
    }
}
