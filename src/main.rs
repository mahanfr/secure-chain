use std::{io::{self, Write}, ops::{Deref, DerefMut}, process::exit, sync::Arc};

use anyhow::Result;
use colored::Colorize;
use tokio::sync::Mutex;

use crate::p2p::{connect, ping_node, start_listener, P2PContext};

mod blockchain;
mod types;
mod block;
mod bootstrap;
mod p2p;

static DEBUG : bool = true;

pub fn qa(question: &str, answer: &mut String) -> io::Result<()> {
    print!("{question} ");
    std::io::stdout().flush()?;

    std::io::stdin().read_line(answer)?;
    Ok(())
}

pub async fn terminal_shell() -> io::Result<()> {
    loop {
        let mut buffer = String::new();
        print!("{} ","$>".blue());
        std::io::stdout().flush()?;

        std::io::stdin().read_line(&mut buffer)?;
        let command = buffer.trim().to_lowercase().to_string();
        match command.as_str() {
            "" => (),
            "exit" => exit(0),
            "start" => {
                let mut buffer = String::new();
                qa("incoming port (default 7070)?", &mut buffer)?;
                let port_str = buffer.trim(); 
                let port = match port_str.parse::<u16>() {
                    Ok(x) => {
                        if x < 1024 {
                            println!("{}: Root privilages needed for ports smaller than 1024","warning".yellow());
                        }
                        x   
                    },
                    Err(e) => {
                        if !port_str.is_empty() {
                            eprintln!("Unexpected port number: {e}");
                        }
                        7070
                    },
                };
                run_server(port).await;
            },
            _ => println!("Unknown command: {}",command),
        }
    }
}

pub async fn run_server(port: u16) {
    let ctx = Arc::new(Mutex::new(P2PContext::new(port)));
    ctx.lock().await.compile_nodes_list(&mut vec![]);

    ping_node(&mut ctx.lock().await.deref_mut());
 
    let ctx1: Arc<Mutex<P2PContext>> = Arc::clone(&ctx);
    let ctx2: Arc<Mutex<P2PContext>> = Arc::clone(&ctx);

    tokio::spawn(async move {
        let _ = start_listener(&ctx1.lock().await.deref()).await;
    });

    tokio::spawn(async move {
       let _ = connect(&ctx2.lock().await.deref()).await;
    });
}

#[tokio::main]
async fn main() -> Result<(),()> {
    let args: Vec<String> = std::env::args().collect();
    if DEBUG { println!("{}","*** Debug mode is ON ***".yellow()); }
    if args.len() < 2 {
        terminal_shell().await.unwrap();
    }
    Ok(())
}
