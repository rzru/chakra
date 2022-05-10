use std::{error::Error, str};

use async_std::{
    io::{self, prelude::BufReadExt, stdout},
    process,
};
use futures::{select, AsyncWriteExt, StreamExt};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, FloodsubMessage, Topic},
    identity,
    ping::{Ping, PingConfig, PingEvent},
    swarm::SwarmEvent,
    Multiaddr, NetworkBehaviour, PeerId, Swarm,
};
use log::{error, info, warn};
use termion::{color, style};

const ADDRESS: &str = "/ip4/0.0.0.0/tcp/0";

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent")]
struct MyBehaviour {
    ping: Ping,
    floodsub: Floodsub,
}

#[derive(Debug)]
enum OutEvent {
    Floodsub(FloodsubEvent),
    Ping(PingEvent),
}

impl From<PingEvent> for OutEvent {
    fn from(v: PingEvent) -> Self {
        Self::Ping(v)
    }
}

impl From<FloodsubEvent> for OutEvent {
    fn from(v: FloodsubEvent) -> Self {
        Self::Floodsub(v)
    }
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let mut connection_established = false;

    info!("Hello, Welcome to Chakra. Enter address of a person or wait for another person to establish connection with you.");
    info!("Your peer id is: {}", local_peer_id);

    let transport = libp2p::development_transport(local_key).await?;

    let topic = Topic::new("chakra-chat");
    let mut floodsub = Floodsub::new(local_peer_id);

    if !floodsub.subscribe(topic.clone()) {
        error!("Cannot subscribe to floodsub topic! Try again later.");
        process::exit(1);
    }

    let behaviour = MyBehaviour {
        ping: Ping::new(PingConfig::new().with_keep_alive(true)),
        floodsub,
    };

    let mut swarm = Swarm::new(transport, behaviour, local_peer_id);
    swarm.listen_on(ADDRESS.parse()?)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    loop {
        select! {
            line = stdin.select_next_some() => {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => {
                        warn!("Something went wrong while reading the line, try again!");
                        continue;
                    },
                };

                if !connection_established {
                    let addr: Multiaddr = match line.parse() {
                        Ok(addr) => addr,
                        Err(_) => {
                            warn!("Entered string is not an address, try again!");
                            continue;
                        },
                    };

                    swarm.dial(addr)?;
                    info!("You will get notice once connection is established.");
                    continue;
                }

                print_prompt(local_peer_id).await;

                swarm
                    .behaviour_mut()
                    .floodsub
                    .publish(topic.clone(), line.as_bytes());
            },
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("One of your possible addresses is {}", address)
                }
                SwarmEvent::Behaviour(OutEvent::Ping(PingEvent { peer, .. })) => {
                    if !connection_established {
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);

                        info!("Connection established with {}", peer);
                        connection_established = true;
                    }
                },
                SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Subscribed { peer_id, .. })) => {
                    info!("Chat started with peer {}. Say something!", peer_id);
                    print_prompt(local_peer_id).await;
                },
                SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(FloodsubMessage { data, source, .. }))) => {
                    println!("\r{}{}", format_prompt(source, color::Magenta), str::from_utf8(&data).unwrap());
                    print_prompt(local_peer_id).await;
                }
                _ => {}
            }
        }
    }
}

fn format_prompt<T>(peer_id: PeerId, color: T) -> String
where
    T: color::Color,
{
    format!(
        "{}{}[{}]:{}{} ",
        style::Bold,
        color::Fg(color),
        peer_id,
        color::Fg(color::Reset),
        style::Reset
    )
}

async fn print_prompt(peer_id: PeerId) {
    print!("{}", format_prompt(peer_id, color::Cyan));
    stdout().flush().await.unwrap();
}
