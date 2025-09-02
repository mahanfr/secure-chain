use std::{io::{BufRead, BufReader, Write}, net::{TcpStream}, sync::{Arc, Mutex}, thread};

use anyhow::Result;
use tokio::{io::AsyncWriteExt, net::TcpListener};

use crate::bootstrap::{read_nodes_list, Node};

pub struct P2PContext {
    pub connection_limit: usize,
    pub nodes: Vec<Node>,
    pub peers: Arc<Mutex<Vec<TcpStream>>>,
    pub serving_addr: String,
}

impl P2PContext {
    pub fn new(port: u16) -> Self {
        Self {
            connection_limit: 8,
            nodes: vec![],
            peers: Arc::new(Mutex::new(vec![])),
            serving_addr: format!("0.0.0.0:{port}"),
        }
    }

    pub fn compile_nodes_list(&mut self, trusted_nodes: &mut Vec<Node>) {
        let mut nodes = read_nodes_list("data/starting_nodes.json");
        nodes.append(trusted_nodes);
        self.nodes = nodes;
    }
}

pub async fn connect(ctx: &P2PContext) -> Result<()> {
    Ok(())
}

pub async fn start_listener(ctx: &P2PContext) -> Result<()> {
   let listener = TcpListener::bind(ctx.serving_addr.clone()).await?;
    println!("server is running on {}",ctx.serving_addr.clone());

    loop {
        let (mut stream, _peer_addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = stream.write(b"egg-nolged\n").await {
                eprintln!("cannot write to stream: {e}");
            }
        });
    }
}

pub fn ping_node(ctx: &mut P2PContext) {
    for node in ctx.nodes.iter_mut() {
        if let Ok(stream) = TcpStream::connect(node.addr()) {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if let Ok(_) = reader.read_line(&mut line) {
                // println!("peer {} sayes {}",node.addr(), line);
                println!("Node {}: stablished a connection", node.addr());
                ctx.peers.lock().unwrap().push(stream);
            }
        } else {
            println!("Node {}: Refused to connect", node.addr());
        }
    }
}
