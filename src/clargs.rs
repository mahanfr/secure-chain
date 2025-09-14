use std::process::exit;

use anyhow::Result;

use crate::{log_error, log_warn, networking::P2PNetwork};

pub fn parse_args(args: &[String], network: &mut P2PNetwork) -> Result<()> {
    for i in 1..args.len() {
        match args[i].as_str() {
            "-p" | "--port" => {
                if i + 1 >= args.len() {
                    anyhow::bail!("Port number not provided");
                }
                let port_str = &args[i + 1];
                if let Ok(port) = port_str.parse::<u16>() {
                    if port < 1024 {
                        log_warn!("Root privilages needed for ports smaller than 1024");
                    }
                    network.set_port(port);
                } else {
                    log_error!("Unexpected port number");
                }
            }
            "-h" | "--h" => {
                print_usage(&args[0]);
                exit(0);
            }
            _ => (),
        }
    }
    Ok(())
}

fn print_usage(program_name: &str) {
    println!("Usage: {} [OPTIONS]\n", program_name);
    println!("Options:");
    println!("  -p, --port PORT    Specify port number (required)");
    println!("  -h, --help         Show this help message");
    println!();
    println!("Example:");
    println!("  {} -p 8080", program_name);
    println!("  {} --port 3000", program_name);
}
