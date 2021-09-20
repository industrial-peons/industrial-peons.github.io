use std::collections::BTreeMap;
use std::env::current_exe;
use std::fs::{metadata, read_dir, read_to_string, remove_file, write};
use std::path::{Path, PathBuf};

use chrono::*;
use chrono_tz::US::Pacific;
use rlua::{Lua, Table};
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(short, long)]
    pub installation_root: String,

    #[structopt(short, long)]
    pub account_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Standing {
    ep: i32,
    gp: i32,
    timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug)]
struct PlayerInfo {
    ep: i32,
    gp: i32,
    log: Vec<Standing>,
}

#[derive(Debug)]
enum Target {
    Unknown,
    Guild,
    Raid,
    Player(String),
}

#[derive(Debug)]
struct LogEntry {
    target: Target,
    description: String,
    standing_change: Option<(Standing, Standing)>,
    timestamp: i64,
}

fn read_str(path: &Path) -> String {
    match read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => panic!("Failed to read '{}': {}", path.to_str().unwrap(), err),
    }
}

fn load_standings(vars_path: &Path) -> BTreeMap<String, PlayerInfo> {
    let lua = Lua::new();
    lua.context(|lua_ctx| {
        let globals = lua_ctx.globals();
        lua_ctx
            .load(&read_str(&vars_path.join("CEPGP-StandingsTracker.lua")))
            .set_name("CEPGP-StandingsTracker.lua")
            .unwrap()
            .exec()
            .unwrap();
        let cepgp_st = globals
            .get::<_, Table>("CEPGP_ST")
            .expect("Unable to load CEPGP-StandingsTracker data.");
        let roster = cepgp_st.get::<_, Table>("Roster").unwrap();
        roster
            .pairs()
            .filter_map(|pair: Result<(String, Table), rlua::Error>| {
                let (member, info) = pair.unwrap();
                let standings = info
                    .get::<_, Table>(9)
                    .unwrap()
                    .sequence_values::<Table>()
                    .map(|standing| {
                        let standing = standing.unwrap();
                        Standing {
                            ep: standing.get(1).unwrap(),
                            gp: standing.get(2).unwrap(),
                            timestamp: Pacific
                                .from_local_datetime(
                                    &NaiveDateTime::parse_from_str(
                                        &standing.get::<_, String>(3).unwrap(),
                                        "%m/%d/%y %H:%M:%S",
                                    )
                                    .unwrap(),
                                )
                                .unwrap()
                                .timestamp(),
                        }
                    })
                    .collect::<Vec<_>>();
                if standings.len() > 1 {
                    let last_standing = standings.last().unwrap();
                    if last_standing.ep == 0 {
                        None
                    } else {
                        Some((
                            member,
                            PlayerInfo {
                                ep: last_standing.ep,
                                gp: last_standing.gp,
                                log: standings,
                            },
                        ))
                    }
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>()
    })
}

fn load_traffic(vars_path: &Path) -> Vec<LogEntry> {
    let item_regex = regex::Regex::new(r"\[.*?\]").unwrap();
    let lua = Lua::new();
    lua.context(|lua_ctx| {
        let globals = lua_ctx.globals();
        lua_ctx
            .load(&read_str(&vars_path.join("CEPGP.lua")))
            .set_name("CEPGP.lua")
            .unwrap()
            .exec()
            .unwrap();
        let cepgp_st = globals
            .get::<_, Table>("CEPGP")
            .expect("Unable to load CEPGP data.");
        let traffic = cepgp_st.get::<_, Table>("Traffic").unwrap();
        traffic
            .sequence_values::<Table>()
            .map(|log_entry| {
                let log_entry = log_entry.unwrap();
                let target = log_entry.get::<_, String>(1).unwrap();
                // Entry 2 is the name of the officer who made the change. Useful
                // for auditing, but not for display.
                let mut description = log_entry.get::<_, String>(3).unwrap();
                let pre_ep = log_entry.get::<_, String>(4).unwrap();
                let pre_gp = log_entry.get::<_, String>(5).unwrap();
                let post_ep = log_entry.get::<_, String>(6).unwrap();
                let post_gp = log_entry.get::<_, String>(7).unwrap();
                let item = log_entry.get::<_, String>(8).unwrap();
                let timestamp = log_entry
                    .get::<_, String>(9)
                    .unwrap()
                    .parse::<i64>()
                    .unwrap();
                // Entry 10 is the timestamp of the transaction plus the amount of
                // time the loot master's computer has been powered on. I have no idea why.
                // Entry 11 is the player ID of the officer who made the change.

                let target = match target.as_str() {
                    "" => Target::Unknown,
                    "Guild" => Target::Guild,
                    "Raid" => Target::Raid,
                    _ => Target::Player(target),
                };
                if let Some(item) = item_regex.find(&item) {
                    description.push_str(&format!("\n{}", item.as_str()))
                };

                let standing_change = match (
                    pre_ep.parse::<i32>(),
                    pre_gp.parse::<i32>(),
                    post_ep.parse::<i32>(),
                    post_gp.parse::<i32>(),
                ) {
                    (Ok(pre_ep), Ok(pre_gp), Ok(post_ep), Ok(post_gp)) => Some((
                        Standing {
                            ep: pre_ep,
                            gp: pre_gp,
                            timestamp,
                        },
                        Standing {
                            ep: post_ep,
                            gp: post_gp,
                            timestamp,
                        },
                    )),
                    _ => None,
                };

                LogEntry {
                    target,
                    description,
                    standing_change,
                    timestamp,
                }
            })
            .collect::<Vec<_>>()
    })
}

fn find_file(name: &str) -> PathBuf {
    let mut path = current_exe().unwrap();
    loop {
        if !path.pop() {
            panic!("Could not find entry for '{}'", name);
        }

        path.push(name);
        if metadata(&path).is_ok() {
            return path;
        }
        path.pop();
    }
}

fn write_data(standings: &BTreeMap<String, PlayerInfo>) {
    let write_dir = find_file("epgp_standings");
    let characters_dir = write_dir.join("players");
    for file in read_dir(&characters_dir).unwrap() {
        remove_file(file.unwrap().path()).unwrap();
    }
    for (character, info) in standings {
        write(
            characters_dir.join(format!("{}.json", character)),
            serde_json::to_string_pretty(&info).unwrap(),
        )
        .unwrap();
    }
    write(
        write_dir.join("_standings_table.md"),
        format!(
            "
| Name | Effort points | Gear points | Priority |
|:-----|:-------------:|:-----------:|---------:|
{}",
            standings
                .iter()
                .map(|(name, info)| format!(
                    "|{}|{}|{}|{:.2}|",
                    name,
                    info.ep,
                    info.gp,
                    info.ep as f64 / info.gp as f64
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .unwrap();
}

pub fn run(opt: &Opt) {
    // PowerShell is an odd duck.
    let cleaned_install_root = opt.installation_root.trim_end_matches("\"");
    let vars_path = Path::new(cleaned_install_root).join(
        [
            "_classic_",
            "WTF",
            "Account",
            &opt.account_name,
            "SavedVariables",
        ]
        .iter()
        .collect::<PathBuf>(),
    );

    let standings = load_standings(&vars_path);
    let traffic = load_traffic(&vars_path);

    write_data(&standings);
}

fn main() {
    run(&Opt::from_args());
}
