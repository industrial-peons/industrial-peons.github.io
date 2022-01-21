use std::collections::{BTreeMap, HashMap};
use std::env::current_exe;
use std::fs::{metadata, read_dir, read_to_string, remove_file, write};
use std::path::{Path, PathBuf};

use chrono::*;
use chrono_tz::US::Pacific;
use regex::Regex;
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
    description: Option<String>,
    timestamp: i64,
}

struct AltMap {
    map: HashMap<String, String>,
}

impl AltMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn set_alts(&mut self, main: &str, alts: Vec<String>) {
        for alt in alts {
            if let Some(existing) = self.map.insert(alt, String::from(main)) {
                panic!("Duplicate alt detected for {} and {}", main, existing)
            };
        }
    }

    pub fn get_main<'a>(&'a self, char: &'a str) -> &'a str {
        self.map.get(char).map(|str| str.as_str()).unwrap_or(char)
    }
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
                    .filter_map(|standing| match standing {
                        Ok(s) => Some(s),
                        Err(e) => {
                            println!("Error parsing standing for {}: {:?}", member, e);
                            None
                        }
                    })
                    .map(|standing| Standing {
                        ep: standing.get(1).unwrap(),
                        gp: standing.get(2).unwrap(),
                        description: None,
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
                    })
                    .collect::<Vec<_>>();
                if standings.len() > 1 {
                    let last_standing = standings.last().unwrap();
                    if last_standing.ep < 100 {
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

fn load_traffic(vars_path: &Path) -> (AltMap, Vec<LogEntry>) {
    let item_regex = Regex::new(r"\[.*?\]").unwrap();
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
        let alts = cepgp_st
            .get::<_, Table>("Alt")
            .unwrap()
            .get::<_, Table>("Links")
            .unwrap()
            .pairs()
            .fold(
                AltMap::new(),
                |mut alt_map, pair: Result<(String, Table), rlua::Error>| {
                    let (main, alts) = pair.unwrap();
                    alt_map.set_alts(
                        &main,
                        alts.sequence_values().collect::<Result<_, _>>().unwrap(),
                    );
                    alt_map
                },
            );
        let traffic = traffic
            .sequence_values::<Table>()
            .map(|log_entry| {
                let log_entry = log_entry.unwrap();
                let target = log_entry.get::<_, String>(1).unwrap();
                // Entry 2 is the name of the officer who made the change. Useful
                // for auditing, but not for display.
                let mut description = log_entry.get::<_, String>(3).unwrap();
                let pre_ep = log_entry.get::<_, String>(4).unwrap();
                let post_ep = log_entry.get::<_, String>(5).unwrap();
                let pre_gp = log_entry.get::<_, String>(6).unwrap();
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
                match &target {
                    Target::Player(player) if alts.get_main(player) != player => {
                        description.push_str(&format!(" (on {})", player))
                    }
                    _ => (),
                }

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
                            description: None,
                            timestamp,
                        },
                        Standing {
                            ep: post_ep,
                            gp: post_gp,
                            description: None,
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
            .collect::<Vec<_>>();

        (alts, traffic)
    })
}

fn set_annotations(
    standings: &mut BTreeMap<String, PlayerInfo>,
    alts: &AltMap,
    traffic: &Vec<LogEntry>,
) {
    let decay_regex = Regex::new(r"Decayed (EP|GP) -(\d+)%").unwrap();
    let raid_ep_regex = Regex::new(r"Add Raid EP \+(\d+)\s+(.*)").unwrap();
    let mut player_logs = standings
        .iter_mut()
        .map(|(name, player_info)| (name.as_str(), (player_info, 0)))
        .collect::<HashMap<_, _>>();

    let skip_until = |player_logs: &mut HashMap<&str, (&mut PlayerInfo, usize)>, timestamp: i64| {
        for (_, (player_info, idx)) in player_logs.iter_mut() {
            while idx < &mut player_info.log.len() && player_info.log[*idx].timestamp < timestamp {
                *idx += 1;
            }
        }
    };

    let annotate_group = |player_logs: &mut HashMap<&str, (&mut PlayerInfo, usize)>,
                          f: &dyn Fn(Option<&Standing>, &Standing) -> bool,
                          description: &str,
                          timestamp: i64| {
        // 15 seconds of grace time.
        for (_, (player_info, idx)) in player_logs.iter_mut() {
            if idx < &mut player_info.log.len()
                && (player_info.log[*idx].timestamp - timestamp).abs() <= 15
                && f(
                    if *idx > 0 {
                        Some(&player_info.log[*idx - 1])
                    } else {
                        None
                    },
                    &player_info.log[*idx],
                )
            {
                player_info.log[*idx].description = Some(description.to_owned());
                *idx += 1;
            }
        }
    };

    for log_entry in traffic {
        // Skip entries more than five minutes older than the current entry. I
        // wouldn't be surprised if either or both of the data sources we're
        // using here are lossy.
        skip_until(&mut player_logs, log_entry.timestamp);

        match &log_entry.target {
            Target::Unknown => {
                // We did our best.
                println!("{}: Unknown annotation", log_entry.timestamp);
            }
            Target::Guild => {
                if let Some(captures) = decay_regex.captures(&log_entry.description) {
                    println!(
                        "{}: Guild annotation {:?}",
                        log_entry.timestamp,
                        (captures[1].to_owned(), captures[2].to_owned())
                    );
                    let decay_type = captures[1].to_owned();
                    let decay_value = captures[2].parse::<i32>().unwrap();
                    annotate_group(
                        &mut player_logs,
                        &|prev, curr| {
                            if let Some(prev) = prev {
                                match decay_type.as_str() {
                                    "EP" => prev.ep > curr.ep,
                                    "GP" => prev.gp > curr.gp,
                                    s => panic!("Unrecognized decay type: '{}'", s),
                                }
                            } else {
                                true
                            }
                        },
                        &format!("{}% weekly {} decay", decay_value, decay_type),
                        log_entry.timestamp,
                    )
                } else {
                    println!("{}: Guild annotation failed parsing", log_entry.timestamp);
                }
            }
            Target::Raid => {
                if let Some(captures) = raid_ep_regex.captures(&log_entry.description) {
                    println!(
                        "{}: Raid annotation {:?}",
                        log_entry.timestamp,
                        (captures[1].to_owned(), captures[2].to_owned())
                    );
                    let ep_gain = captures[1].parse::<i32>().unwrap();
                    let source = captures[2].to_owned();
                    let source_string = source
                        .strip_prefix("- ")
                        .map(|boss| format!("(Killed {})", boss))
                        .unwrap_or(source);
                    annotate_group(
                        &mut player_logs,
                        &|prev, curr| {
                            if let Some(prev) = prev {
                                prev.ep + ep_gain == curr.ep
                            } else {
                                true
                            }
                        },
                        &format!("Add EP {} {}", ep_gain, source_string),
                        log_entry.timestamp,
                    )
                } else {
                    println!("{}: Raid annotation failed parsing", log_entry.timestamp);
                }
            }
            Target::Player(player) => {
                let player = alts.get_main(&player);
                if player_logs.contains_key(player) {
                    let (player_info, idx) = &mut player_logs.get_mut(player).unwrap();
                    if *idx < player_info.log.len()
                        && (player_info.log[*idx].timestamp - log_entry.timestamp).abs() <= 15
                    {
                        println!(
                            "{}: Found a probable event for player '{}'",
                            log_entry.timestamp, player
                        );
                        if *idx == 0
                            || (match &log_entry.standing_change {
                                Some((pre, post)) => {
                                    player_info.log[*idx - 1].ep == pre.ep
                                        && player_info.log[*idx - 1].gp == pre.gp
                                        && player_info.log[*idx].ep == post.ep
                                        && player_info.log[*idx].gp == post.gp
                                }
                                None => true,
                            })
                        {
                            player_info.log[*idx].description =
                                Some(log_entry.description.to_owned());
                            *idx += 1;
                        } else {
                            println!("\t-> Could not attribute event");
                            println!(
                                "\t-> {:#?}",
                                (
                                    &log_entry.standing_change,
                                    &player_info.log[*idx - 1],
                                    &player_info.log[*idx]
                                )
                            );
                        }
                    } else {
                        println!(
                            "{}: Could not find a probable event for player '{}'",
                            log_entry.timestamp, player
                        );
                    }
                } else {
                    println!("{}: Player '{}' not found", log_entry.timestamp, player);
                }
            }
        }
    }
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
_Updated {}._

| Name | Effort points | Gear points | Priority |
|:-----|:-------------:|:-----------:|---------:|
{}",
            chrono::Local::today().format("%m/%d/%Y"),
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

    let (alts, traffic) = load_traffic(&vars_path);
    let mut standings = load_standings(&vars_path)
        .into_iter()
        .filter(|(char, _)| char == alts.get_main(char))
        .collect::<BTreeMap<_, _>>();
    set_annotations(&mut standings, &alts, &traffic);

    write_data(&standings);
}

fn main() {
    run(&Opt::from_args());
}
