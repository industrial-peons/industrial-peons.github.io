use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

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
struct TrafficEntry {
    target: String,
    actor: String,
    comment: String,
    pre_ep: Option<i32>,
    post_ep: Option<i32>,
    pre_gp: Option<i32>,
    post_gp: Option<i32>,
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
    let (cepgp_standings, guild_log) = lua.context(|lua_ctx| {
        let globals = lua_ctx.globals();
        lua_ctx
            .load(&read_str(&vars_path.join("CEPGP.lua")))
            .set_name("CEPGP.lua")
            .unwrap()
            .exec()
            .unwrap();
        lua_ctx
            .load(&read_str(&vars_path.join("CEPGP-StandingsTracker.lua")))
            .set_name("CEPGP-StandingsTracker.lua")
            .unwrap()
            .exec()
            .unwrap();

        let cepgp = globals
            .get::<_, Table>("CEPGP")
            .expect("Unable to load CEPGP data.");
        let traffic = cepgp
            .get::<_, Table>("Traffic")
            .unwrap()
            .sequence_values::<Table>()
            .map(|inner| {});

        let cepgp_st = globals
            .get::<_, Table>("CEPGP_ST")
            .expect("Unable to load CEPGP-StandingsTracker data.");
        let roster = cepgp_st.get::<_, Table>("Roster").unwrap();

        (0, 0)
    });
}
