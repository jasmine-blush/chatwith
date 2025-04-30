use core::fmt;
use curl::easy::Easy;
use curl::multi::Easy2Handle;
use dirs;
use serde_json::Value;
use std::error::Error;
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::io::stdout;
use std::path::PathBuf;

const VALID_COMMANDS: [&str; 5] = ["help", "entry", "remove", "show", "list"];

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
            config = parse_config(config, fs::read_to_string(&cfg_path)?.lines().collect())?;
        }

        match query.command.as_str() {
            "entry" => {
                entry(&query.args, &mut config)?;
                update_config(&config, &cfg_path)?;
            }
            "remove" => {
                config = remove(&query.args, &config);
                update_config(&config, &cfg_path)?;
            }
            "show" => show(&query.args, &config),
            "list" => list(&config),
            _ => chat(&query, &config),
        }
    } else {
        help()
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    model: String,
    options: Vec<String>,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} {}", self.name, self.model, self.options.join(" "))
    }
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

fn update_config(config: &Vec<Entry>, path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let mut file: File = File::create(path)?;

    let mut cfg_string: String = String::new();
    for entry in config {
        let line: String = format!("{}\n", entry);
        file.write_all(line.as_bytes())?;
    }

    Ok(())
}

fn help() {
    println!("Valid commands: help, add, update, remove, show, list, <entry_name>");
}

fn entry(args: &Vec<String>, config: &mut Vec<Entry>) -> Result<(), Box<dyn Error>> {
    if args.len() >= 2 {
        let options: Vec<String> = match args.len() {
            2 => Vec::new(),
            _ => args[2..].to_vec(),
        };

        let new_entry: Entry = Entry {
            name: args[0].clone(),
            model: args[1].clone(),
            options,
        };

        if config.iter().any(|entry| entry.name == new_entry.name) {
            let mut count: i32 = 0;
            for entry in config.iter_mut() {
                if entry.name == new_entry.name {
                    entry.model = new_entry.model.clone();
                    entry.options = new_entry.options.clone();
                    count += 1;
                }
            }
            if count > 1 {
                println!("Updated {count} entries.");
            } else {
                println!("Updated 1 entry.");
            }
        } else {
            config.push(new_entry);
            println!("Entry successfully added.");
        }
    } else {
        Err("Incomplete entry given. Please provide at least a name and a model.")?;
    }

    Ok(())
}

fn remove(args: &Vec<String>, config: &Vec<Entry>) -> Vec<Entry> {
    let mut new_config: Vec<Entry> = config.clone();

    if args.len() > 0 {
        for arg in args {
            new_config.retain(|entry| entry.name != *arg);
        }
    }

    println!(
        "Removed {} entries from config file.",
        config.len() - new_config.len()
    );

    new_config
}

fn show(args: &Vec<String>, config: &Vec<Entry>) {
    if config.len() > 0 {
        for entry in config {
            if args.iter().any(|arg| *arg == entry.name) {
                println!("{}", entry);
            }
        }
    }
}

fn list(config: &Vec<Entry>) {
    if config.len() > 0 {
        println!("{} entries found in config file:", config.len());
        for entry in config {
            println!("{}", entry);
        }
    } else {
        println!("No entries found in config file.");
    }
}

fn chat(query: &Query, config: &Vec<Entry>) {
    let query_string = format!(
        "{}{}{}{}{}",
        r#"{"model":""#,
        config
            .iter()
            .find(|entry| entry.name == query.command)
            .expect("test0")
            .model,
        r#"","messages":[{"role":"user","content":""#,
        query.args.join(" "),
        r#""}],"stream":true}"#
    );
    let mut data = query_string.as_bytes();
    let mut easy = Easy::new();
    easy.url("http://localhost:11434/api/chat").unwrap();
    easy.post(true).unwrap();

    easy.post_fields_copy(data).unwrap();
    easy.write_function(|data| {
        let json: Value =
            serde_json::from_str(String::from_utf8(data.to_vec()).unwrap().as_str()).unwrap();
        let mut output: String = json
            .get("message")
            .expect("test")
            .get("content")
            .expect("test2")
            .to_string()
            .replace("\"", "");
        let newlines: usize = output.matches("\\n").count();
        if newlines > 0 {
            output = output.replace("\\n", "");
        }

        print!("{}", output);
        for _ in 0..newlines {
            println!();
        }
        stdout().flush();
        Ok(data.len())
    })
    .unwrap();
    easy.perform().unwrap();
}
