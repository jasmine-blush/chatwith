use core::fmt;
use curl::easy::Easy;
use curl::easy::WriteError;
use curl::multi::Easy2Handle;
use dirs;
use serde_json::Value;
use std::cell::RefCell;
use std::error::Error;
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::io::stdout;
use std::path::PathBuf;
use std::rc::Rc;

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
            Some(path) => path.join("chatwith/chatwith.cfg"),
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
            _ => chat(&query, &config)?,
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

fn chat(query: &Query, config: &Vec<Entry>) -> Result<(), Box<dyn Error>> {
    match config.iter().find(|entry| entry.name == query.command) {
        Some(entry) => {
            let model: &String = &entry.model;

            let mut start_index: usize = 0;
            if query.args.len() > 0 {
                if query.args[0] == "-n" {
                    remove_conversation(model);
                    start_index = 1;
                }
            }
            let mut conversation: Conversation = get_conversation(model)?;
            conversation.messages.push(Message {
                role: Role::User,
                content: query.args[start_index..].join(" "),
            });
            let response: String = send_message(&conversation)?;
            conversation.messages.push(Message {
                role: Role::Assistant,
                content: response,
            });
            update_conversation(&conversation);
        }
        None => {
            println!("No model with name {} found in config file.", query.command);
        }
    }
    Ok(())
}

fn send_message(conversation: &Conversation) -> Result<String, Box<dyn Error>> {
    let request_string: String = conversation.to_json_string();
    let request = request_string.as_bytes();
    let mut easy = Easy::new();
    easy.url("http://localhost:11434/api/chat")?;
    easy.post(true)?;

    let response = Rc::new(RefCell::new(String::new()));
    let response_clone = response.clone();
    let mut is_response: bool = true; // indicates whether thinking-block has ended

    let first_bold: bool = true;
    easy.post_fields_copy(request)?;
    let mut transfer = easy.transfer();
    transfer.write_function(|data: &[u8]| {
        let json: Value =
            serde_json::from_str(String::from_utf8(data.to_vec()).unwrap().as_str()).unwrap();
        let mut output = match json.get("message").and_then(|msg| msg.get("content")) {
            Some(content) => content.to_string().replace("\"", ""),
            None => {
                eprintln!(
                    "No value message.content in response json. Response is: {}",
                    json
                );
                return Ok(data.len());
            }
        };
        let newlines: usize = output.matches("\\n").count();
        if newlines > 0 {
            output = output.replace("\\n", "").replace("\\", ""); // sanitize newlines and escape slashes
        }

        if output.contains("<think>") {
            print!("\x1B[90m");
            is_response = false;
        }

        print!("{}", output);
        if is_response {
            response_clone.borrow_mut().push_str(&output);
        }

        if output.contains("</think>") {
            print!("\x1B[39m");
            is_response = true;
        }

        for _ in 0..newlines {
            println!();
            if is_response {
                response_clone.borrow_mut().push_str("\n");
            }
        }

        stdout().flush();
        Ok(data.len())
    })?;
    transfer.perform()?;

    Ok(response.borrow().clone())
}

struct Conversation {
    model: String,
    messages: Vec<Message>,
}

impl Conversation {
    fn to_json_string(&self) -> String {
        let mut json_string: String =
            format!("{}{}{}", r#"{"model":""#, self.model, r#"","messages":["#);
        for message in &self.messages {
            json_string.push_str(&message.to_json_string());
            json_string.push(',');
        }
        json_string.pop();
        json_string.push_str(r#"],"stream":true}"#);
        json_string
    }
}

#[derive(Debug)]
struct Message {
    role: Role,
    content: String,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<{}>\n{}\n</{}>\n", self.role, self.content, self.role)
    }
}

impl Message {
    fn to_json_string(&self) -> String {
        format!(
            "{}{}{}{}{}",
            r#"{"role":""#, self.role, r#"","content":""#, self.content, r#""}"#
        )
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Role {
    User,
    Assistant,
    None,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::None => write!(f, "none"),
        }
    }
}

fn remove_conversation(model: &String) -> Result<(), Box<dyn Error>> {
    let conversation_path: PathBuf = match dirs::config_dir() {
        Some(path) => path.join("chatwith/").join(format!("{}{}", model, ".conv")),
        None => Err("No valid config path found in environment variables.")?,
    };

    let mut file_result: Result<File, std::io::Error> =
        File::options().write(true).open(conversation_path);
    if let Ok(mut file) = file_result {
        file.set_len(0)?;
        file.flush();
    }

    Ok(())
}

fn get_conversation(model: &String) -> Result<Conversation, Box<dyn Error>> {
    let conversation_path: PathBuf = match dirs::config_dir() {
        Some(path) => path.join("chatwith/").join(format!("{}{}", model, ".conv")),
        None => Err("No valid config path found in environment variables.")?,
    };

    if conversation_path.try_exists()? {
        return Ok(parse_conversation(
            model,
            fs::read_to_string(&conversation_path)?.lines().collect(),
        ));
    }

    Ok(Conversation {
        model: model.clone(),
        messages: Vec::new(),
    })
}

fn parse_conversation(model: &String, lines: Vec<&str>) -> Conversation {
    let mut conversation: Conversation = Conversation {
        model: model.clone(),
        messages: Vec::new(),
    };

    let mut current_role: Role = Role::None;
    for line in lines {
        match line {
            "<user>" => current_role = Role::User,
            "</user>" => current_role = Role::None,
            "<assistant>" => current_role = Role::Assistant,
            "</assistant>" => current_role = Role::None,
            _ => {
                if current_role != Role::None {
                    if conversation.messages.len() == 0 {
                        conversation.messages.push(Message {
                            role: current_role.clone(),
                            content: String::new(),
                        });
                    }

                    if conversation
                        .messages
                        .last()
                        .is_some_and(|message| message.role == current_role)
                    {
                        if let Some(message) = conversation.messages.last_mut() {
                            message.content.push_str(line);
                        }
                    } else {
                        conversation.messages.push(Message {
                            role: current_role.clone(),
                            content: String::from(line),
                        });
                    }
                }
            }
        };
    }

    conversation
}

fn update_conversation(conversation: &Conversation) -> Result<(), Box<dyn Error>> {
    let conversation_path: PathBuf = match dirs::config_dir() {
        Some(path) => path
            .join("chatwith/")
            .join(format!("{}{}", &conversation.model, ".conv")),
        None => Err("No valid config path found in environment variables.")?,
    };

    let mut file: File = File::create(conversation_path)?;
    for message in &conversation.messages {
        file.write_all(
            message
                .to_string()
                .replace(r#"\\'"#, r#"'"#) // remove possible pre-existing sanitization
                .replace(r#"'"#, r#"\\'"#) // sanitize apostrophes
                .as_bytes(),
        )?;
    }
    file.flush();

    Ok(())
}
