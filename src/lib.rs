use dirs;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

const VALID_COMMANDS: [&str; 6] = ["help", "add", "update", "remove", "show", "list"];

#[derive(Debug)]
pub struct Query {
    pub command: String,
    pub args: Vec<String>,
}

impl Query {
    pub fn build(mut args: impl Iterator<Item = String>) -> Self {
        args.next();

        let command: String;

        match args.next() {
            Some(cmd) => command = cmd.to_lowercase(),
            None => command = String::from("help"),
        }

        Self {
            command,
            args: args.collect(),
        }
    }
}

pub fn run(query: &Query) -> Result<(), Box<dyn Error>> {
    let mut config: Vec<Entry> = Vec::new();
    if query.command != "help" {
        let cfg_path: PathBuf = match dirs::config_dir() {
            Some(path) => path.join("chatwith"),
            None => Err("No valid config path found in environment variables.")?,
        };

        if cfg_path.try_exists()? {
            config = parse_config(config, fs::read_to_string(cfg_path)?.lines().collect())?;
        }
    }
    dbg!(&config);
    match query.command.as_str() {
        "help" => help(),
        "add" => {
            add(&query.args, &config);
            update_config(&config);
        }
        "update" => {
            update(&query.args, &config);
            update_config(&config);
        }
        "remove" => {
            config = remove(&query.args, &config);
            update_config(&config);
        }
        "show" => show(&query.args, &config),
        "list" => list(&config),
        _ => run_model(&query, &config),
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    model: String,
    options: Vec<String>,
}

fn parse_config(config: Vec<Entry>, lines: Vec<&str>) -> Result<Vec<Entry>, Box<dyn Error>> {
    let mut config: Vec<Entry> = Vec::new();

    for line in lines {
        let tokens: Vec<&str> = line.split_whitespace().collect::<Vec<&str>>();

        match tokens.len() {
            0 => continue,
            1 => {
                Err(format!(
                    "Invalid entry in config. Make sure to specify a model. Line:\n{line}"
                ))?;
            }
            _ => {
                if VALID_COMMANDS.iter().any(|cmd| *cmd == tokens[0]) {
                    Err(format!(
                        "Invalid entry in config. Make sure the entry is not named after a valid command. Line:\n{line}"
                    ))?;
                } else {
                    config.push(Entry {
                        name: String::from(tokens[0]),
                        model: String::from(tokens[1]),
                        options: tokens[2..].iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }
    }

    Ok(config)
}

fn update_config(config: &Vec<Entry>) {}

fn help() {
    println!("Valid commands: help, add, update, remove, show, list, <entry_name>");
}

fn add(args: &Vec<String>, config: &Vec<Entry>) {}

fn update(args: &Vec<String>, config: &Vec<Entry>) {}

fn remove<'a>(args: &Vec<String>, config: &Vec<Entry>) -> Vec<Entry> {
    let mut new_config: Vec<Entry> = config.clone();

    if args.len() > 0 {
        for arg in args {
            new_config.retain(|entry| entry.name.to_lowercase() != *arg.to_lowercase());
        }
    }

    println!(
        "Removed {} entries from config file.",
        config.len() - new_config.len()
    );

    new_config
}

fn show(args: &Vec<String>, config: &Vec<Entry>) {}

fn list(config: &Vec<Entry>) {
    if config.len() > 0 {
        println!("{} entries found in config file:", config.len());
        for entry in config {
            println!("{} {} {}", entry.name, entry.model, entry.options.join(" "));
        }
    } else {
        println!("No entries found in config file.");
    }
}

fn run_model(query: &Query, config: &Vec<Entry>) {}
