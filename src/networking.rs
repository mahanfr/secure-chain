use core::str;
use std::{
    collections::HashMap,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use bincode::config;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{tcp::{OwnedReadHalf, OwnedWriteHalf}, TcpListener, TcpStream},
    sync::{
        mpsc::{self, Receiver, Sender}, Mutex, RwLock
    },
};

use crate::{
    block::Block, blockchain::Blockchain, errors::NetworkError, keys::PublicKey, log_error,
    log_info, log_warn, peers::Peer, protocol::{header::{ContentType, P2ProtHeader}, P2Protocol},
};

pub type PeerId = u64;
pub type SharedPeers = Arc<Mutex<HashMap<PeerId, Peer>>>;
pub type BootstapNodes = Vec<Peer>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandShake {
    pub pk: PublicKey,
    pub addr: SocketAddr,
}

impl HandShake {
    pub fn new(pk: PublicKey, addr: SocketAddr) -> Self {
        Self { pk, addr, }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandShakeAck {
    pub pk: PublicKey,
    pub peer_id: u64,
    pub timestamp: u128,
    pub addr: SocketAddr,
}

impl HandShakeAck {
    pub fn new(pk: PublicKey, addr: SocketAddr) -> Self {
        Self {
            pk,
            addr,
            peer_id: rand::thread_rng().r#gen(),
            timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u16)]
pub enum PeerMessage {
    Handshake(HandShake),
    HandshakeAck(HandShakeAck),
    GetBlockByHash(String),
    GetBlockById(String),
    GetLastBlock,
    Block(Block),
    Error(NetworkError),
    Ping,
    Pong,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AppState {
    pub pk: PublicKey,
    pub addr: SocketAddr,
    pub chain: RwLock<Blockchain>,
    pub peers: SharedPeers,
}
impl AppState {
    pub fn new(pk: PublicKey, addr: SocketAddr, chain: Blockchain) -> Arc<Self> {
        Arc::new(Self {
            pk,
            addr,
            chain: RwLock::new(chain),
            peers: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[derive(Debug)]
pub enum ClientCommand {
    Broadcast(PeerMessage),
    Ping(PeerId),
    Pong(PeerId),
    ConnectToPeer(Peer),
    NewServerConnection(SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>),
    NewClientConnection(SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>),
    HandShakeAck(SocketAddr, HandShake),
    EstablishedPeerConnection(SocketAddr, Peer, PeerId)
}

#[derive(Debug)]
pub struct P2PNetwork {
    pub listen_addr: SocketAddr,
    pub bootstrap_nodes: Vec<Peer>,
    client_tx: Option<Arc<Sender<ClientCommand>>>,
}

impl P2PNetwork {
    pub fn new(listen_port: u16, bootstrap_nodes: BootstapNodes) -> Self {
        let listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", listen_port)).unwrap();
        Self {
            listen_addr,
            bootstrap_nodes,
            client_tx: None,
        }
    }

    pub fn set_port(&mut self, port: u16) {
        self.listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", port)).unwrap();
    }

    pub async fn start(&mut self, state: Arc<AppState>) -> Result<()> {
        let (client_tx, client_rx) = mpsc::channel(100);
        let tx = Arc::new(client_tx);
        self.client_tx = Some(tx.clone());

        let listener = TcpListener::bind(self.listen_addr).await?;
        log_info!("P2P node listening on {}", self.listen_addr);

        let state_cloned = state.clone();
        let tx_cloned = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::accept_connections(state_cloned, tx_cloned, listener).await {
                log_error!("Error accepting connection: {e}");
            }
        });

        let state_cloned = state.clone();
        let tx_cloned = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::start_client(state_cloned, tx_cloned, client_rx).await {
                log_error!("Error connecting to peers: {e}");
            }
        });

        let state_cloned = state.clone();
        let tx_cloned = tx.clone();
        if let Err(e) = self.connect_to_bootstrap_nodes(state_cloned, tx_cloned).await {
            log_error!("Error Connecting to bootstrap nodes: {e}");
        }

        Ok(())
    }

    async fn accept_connections(
        state: Arc<AppState>,
        client_tx: Arc<Sender<ClientCommand>>,
        listener: TcpListener,
    ) -> Result<()> {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log_info!("New connection from {addr}");

                    let (r, w) = stream.into_split();
                    let reader = BufReader::new(r);
                    let writer = BufWriter::new(w);
                    client_tx.send(ClientCommand::NewServerConnection(addr, Arc::new(Mutex::new(writer)))).await?;

                    let state = state.clone();
                    let client_tx = client_tx.clone();
                    tokio::spawn(async move {
                        match Self::handle_incomming(state, reader, client_tx, addr.clone()).await {
                            Ok(_) => {
                                log_warn!("Connection to {addr} closed!");
                            },
                            Err(e) => {
                                log_error!("Error in handeling the connection: {e}");
                            }
                        }
                    });
                }
                Err(e) => {
                    log_error!("Error reciving the connection: {e}");
                }
            }
        }
    }

    async fn handle_incomming(
        _state: Arc<AppState>,
        mut reader: BufReader<OwnedReadHalf>,
        sender: Arc<Sender<ClientCommand>>,
        addr: SocketAddr,
    ) -> Result<()> {

        let mut data: String = String::new();
        let mut header_buf = vec![0u8; 32];
        loop {
            if let Err(e) = reader.read_exact(&mut header_buf).await {
                log_error!("Connection closed or error: {e}");
                break;
            }
            let header = match P2ProtHeader::from_bytes(&header_buf) {
                Ok(h) => h,
                Err(e) => {
                    log_error!("Error parsing the header: {e}");
                    break;
                }
            };
            // DO SOME CHECKS ON HEADER
            if header.content_type != ContentType::HandShake {
                log_error!("Incomming message had no valid session id");
                break;
            }
            // DO SOME CHECKS ON HEADER
            let mut payload = vec![0u8; header.size];
            if let Err(e) = reader.read_exact(&mut payload).await {
                log_error!("Error reading the message: {e}");
                break;
            }
            println!("payload: {:?}",payload);
            let (incoming_msg, _) : (PeerMessage, _) = 
                match bincode::serde::decode_from_slice(&payload, config::standard()) {
                    Ok(m) => m,
                    Err(e) => {
                        log_error!("Error Parsing the message: {e}");
                        break;
                    },
            };
            match incoming_msg {
                PeerMessage::Handshake(recv_hs) => {
                    log_info!("Handshake recived from {addr}");
                    if let Err(e) = sender.send(ClientCommand::HandShakeAck(addr, recv_hs)).await {
                        log_error!("unable to communicate with client service: {e}");
                        break;
                    }
                }
                PeerMessage::HandshakeAck(recv_hs) => {
                    let peer = Peer::new(recv_hs.addr.clone(), recv_hs.pk.clone()); 
                    if let Err(e) = sender.send(ClientCommand::EstablishedPeerConnection(addr, peer, recv_hs.peer_id)).await {
                        log_error!("unable to communicate with client service: {e}");
                        break;
                    }
                }
                PeerMessage::Ping => {
                    log_info!("Ping recived from {addr}");
                    if let Err(e) = sender.send(ClientCommand::Pong(header.session_id)).await {
                        log_error!("unable to communicate with client service: {e}");
                        break;
                    }
                }
                PeerMessage::Pong => {
                    log_info!("Pong recived from {addr}");
                }
                PeerMessage::Error(e) => {
                    log_warn!("Error recived from {addr}: {e}");
                }
                _ => (),
            }
            data.clear();
        }

        Ok(())
    }

    pub async fn connect_to_bootstrap_nodes(&self, state: Arc<AppState> ,client_tx: Arc<Sender<ClientCommand>>) -> Result<()> {
        let nodes = self.bootstrap_nodes.clone();
        for node in nodes {
            if node.addr != self.listen_addr {
                let state = state.clone();
                let client_tx = client_tx.clone();
                tokio::spawn(async move {
                    let mut attempt = 0;
                    loop {
                        if attempt > 3 {
                            break;
                        }
                        match Self::connect_to_server(state.clone(), client_tx.clone(), node.addr).await {
                            Ok(w_buf) => {
                                let _ = client_tx.send(ClientCommand::NewClientConnection(node.addr, Arc::new(Mutex::new(w_buf)))).await;
                                break;
                            }
                            Err(e) => {
                                log_warn!("Error connecting to bootstrap peer {}: {e}",node.addr);
                                attempt += 1;
                            }
                        }
                    }
                });
            }
        }
        Ok(())
    }


    async fn send(writer: Arc<Mutex<BufWriter<OwnedWriteHalf>>>, prot: &P2Protocol) -> Result<()> {
        writer.lock().await.write_all(&prot.serialize()?).await?;
        writer.lock().await.flush().await?;
        Ok(())
    }

    async fn start_client(state: Arc<AppState>, client_tx: Arc<Sender<ClientCommand>> ,mut rx: Receiver<ClientCommand>) -> Result<()> {
        let peers = state.peers.clone();
        let mut connections: HashMap<PeerId, Arc<Mutex<BufWriter<OwnedWriteHalf>>>> = HashMap::new();
        let mut queue_conn: HashMap<SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>>= HashMap::new();

        let tx = client_tx.clone();
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::NewServerConnection(addr, writer) => {
                    queue_conn.insert(addr, writer);
                },
                ClientCommand::NewClientConnection(addr, writer) => {
                    let handshake = HandShake::new(state.pk.clone(), state.addr);
                    let prot = P2Protocol::new(&PeerMessage::Handshake(handshake));
                    if let Err(e) = Self::send(writer.clone(), &prot).await {
                        log_error!("Can not send message to {addr}: {e}");
                    } else {
                        queue_conn.insert(addr, writer);
                    }
                }
                ClientCommand::EstablishedPeerConnection(addr, peer, peer_id) => {
                    if let Some(writer) = queue_conn.remove(&addr) {
                        peers.lock().await.insert(peer_id, peer);
                        connections.insert(peer_id, writer);
                    }
                },
                ClientCommand::HandShakeAck(addr, hs) => {
                    let handshake_ack = HandShakeAck::new(state.pk.clone(), state.addr);
                    if let Some(writer) = queue_conn.remove(&addr) {
                        log_info!("sending handshake to {}", addr);
                        let prot = P2Protocol::new(&PeerMessage::HandshakeAck(handshake_ack.clone()));
                        if let Err(e) = 
                            Self::send(writer.clone(), &prot).await {
                            log_error!("Can not send message to {addr}: {e}");
                        } else {
                            peers.lock().await.insert(handshake_ack.peer_id, Peer { addr: addr.clone(), pk: hs.pk.clone() });
                            connections.insert(handshake_ack.peer_id, writer);
                        }
                    }
                },
                ClientCommand::Pong(id) => {
                    if let Some(conn) = connections.get(&id) {
                        let prot = P2Protocol::new_with_peer(&PeerMessage::Pong, id);
                        if let Err(e) = Self::send(conn.clone(), &prot).await {
                            log_info!("failed sending pong to {id}:{e}");
                        }
                    } else {
                        log_warn!("peer with id {id} not found!");
                    }
                } 
                ClientCommand::Ping(id) => {
                    if let Some(conn) = connections.get(&id) {
                        let prot = P2Protocol::new_with_peer(&PeerMessage::Ping, id);
                        if let Err(e) = Self::send(conn.clone(), &prot).await {
                            log_info!("failed sending ping to {id}:{e}");
                        }
                    }
                }
                ClientCommand::ConnectToPeer(peer) => {
                    match Self::connect_to_server(state.clone(), tx.clone(), peer.addr).await {
                        Ok(w_buf) => {
                            queue_conn.insert(peer.addr, Arc::new(Mutex::new(w_buf)));
                        }
                        Err(e) => {
                            log_warn!("Failed to connect to {}: {e}", peer.addr);
                        }
                    }
                },
                ClientCommand::Broadcast(message) => {
                    for (pid, peer) in peers.lock().await.iter() {
                        if !connections.contains_key(pid) {
                            match Self::connect_to_server(state.clone(), tx.clone(), peer.addr).await {
                                Ok(w_buf) => {
                                    connections.insert(*pid, Arc::new(Mutex::new(w_buf)));
                                }
                                Err(e) => {
                                    log_warn!("Failed to connect to {pid:?}@{}: {e}", peer.addr);
                                }
                            }
                        }
                    }
                    let s = serde_json::to_string(&message)?;
                    let msg = format!("{s}\n");

                    let mut broken = Vec::new();
                    for (pk, peer) in &connections {
                        let mut gaurd = peer.lock().await;
                        if let Err(e) = gaurd.write_all(msg.as_bytes()).await {
                            log_warn!("Failed to write to {pk:?}: {e}");
                            broken.push(pk.clone());
                            continue;
                        }
                        if let Err(e) = gaurd.flush().await {
                            log_warn!("Failed to flush to {pk:?}: {e}");
                            broken.push(pk.clone());
                        }
                    }
                    for pk in broken {
                        connections.remove(&pk);
                    }
                }
            }
        }
        log_warn!("client task ended: channel closed");
        Ok(())
    }

    async fn connect_to_server(state: Arc<AppState>, client_tx: Arc<Sender<ClientCommand>>, addr: SocketAddr) -> Result<BufWriter<OwnedWriteHalf>> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let (r, w) = stream.into_split();
        let reader = BufReader::new(r);
        let writer = BufWriter::new(w);

        tokio::spawn(async move {
            match Self::handle_incomming(state, reader, client_tx ,addr.clone()).await {
                Ok(_) => {
                    log_warn!("Connection with {addr} closed");
                },
                Err(e) => {
                    log_error!("Error in handeling the connection: {e}");
                }
            }
        });
        Ok(writer)
    }

    pub async fn broadcast(&self, message: PeerMessage) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::Broadcast(message)).await?;
        Ok(())
    }

    pub async fn ping(&self, peer_id: u64) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::Ping(peer_id)).await?;
        Ok(())
    }

    pub async fn connect_to_peer(&self, peer: Peer) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::ConnectToPeer(peer)).await?;
        Ok(())
    }

}
