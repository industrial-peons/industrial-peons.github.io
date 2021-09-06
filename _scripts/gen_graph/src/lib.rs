use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use chrono::*;
use rlua::{Lua, Table};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(short, long)]
    pub installation_root: String,

    #[structopt(short, long)]
    pub account_name: String,
}

#[derive(Debug)]
struct Standing {
    ep: i32,
    gp: i32,
    timestamp: NaiveDateTime,
}

fn read_str(path: &Path) -> String {
    match read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => panic!("Failed to read '{}': {}", path.to_str().unwrap(), err),
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
            .map(|pair: Result<(String, Table), rlua::Error>| {
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
                            timestamp: NaiveDateTime::parse_from_str(
                                &standing.get::<_, String>(3).unwrap(),
                                "%m/%d/%y %H:%M:%S",
                            )
                            .unwrap(),
                        }
                    })
                    .collect::<Vec<_>>();
                if member == "Rannveig" {
                    println!("{:#?}", standings);
                }
                (member, standings)
            })
            .collect::<Vec<_>>()
    });
}
