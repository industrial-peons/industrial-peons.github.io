use structopt::StructOpt;

use gen_graph::{run, Opt};

fn main() {
    run(&Opt::from_args());
}
