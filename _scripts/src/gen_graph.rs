use std::collections::BTreeMap;
use std::env::current_exe;
use std::fs::{metadata, read_to_string, write};
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

fn read_str(path: &Path) -> String {
    match read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => panic!("Failed to read '{}': {}", path.to_str().unwrap(), err),
    }
}

fn find_entry(name: &str) -> PathBuf {
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
    let lua = Lua::new();
    let standings = lua.context(|lua_ctx| {
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
    });

    let write_dir = find_entry("epgp_standings");
    write(
        write_dir.join("standings.json"),
        serde_json::to_string_pretty(&standings).unwrap(),
    )
    .unwrap();
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

fn main() {
    run(&Opt::from_args());
}
