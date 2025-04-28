use std::{env, process};

use chatwith::Query;

fn main() {
    let query: Query = Query::build(env::args());

    if let Err(e) = chatwith::run(&query) {
        eprintln!("Error: {e}");
        process::exit(1);
    };
}
