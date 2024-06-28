<h2>
  RFlow
  <a href="https://crates.io/crates/rflow"><img alt="crates.io page" src="https://img.shields.io/crates/v/rflow.svg"></img></a>
  <a href="https://docs.rs/rflow"><img alt="docs.rs page" src="https://docs.rs/rflow/badge.svg"></img></a>
</h2>

Chat-like HMI for embedded Rust applications and PLCs

RFlow is a part of [RoboPLC](https://www.roboplc.com) project.

## The idea

Many of embedded applications and PLC programs do not have any human-machine
interface and not supposed to have one by design.

However, sometimes it is very useful to have a simple way to interact with the
application, e.g. for debugging purposes or having a basic emergency interface
in production.

RFlow provides the most possible lightweight way to have a chat-like interface
between the application (server) and its clients, which does not affect the
application real-time run-flow and consumes minimal system resources.

The [RFlow protocol](https://github.com/roboplc/rflow/blob/main/protocol.md) is
fully text-based and can be used with no special client.

## Clients

* [RFlow Chat](https://crates.io/crates/rflow-chat) - a dedicated RFlow chat
  client (terminal).

* Custom clients, built with the crate `Client` API.

* Any terminal TCP client, e.g. `telnet`, `nc`.

## A very basic example

```rust,no_run
use std::{thread, time::Duration};

use rtsc::time::interval;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_channel = rflow::take_data_channel()?;
    thread::spawn(move || {
        rflow::serve("127.0.0.1:4001").expect("failed to start server");
    });
    thread::spawn(move || {
        for _ in interval(Duration::from_secs(5)) {
            rflow::send("ping".to_string());
        }
    });
    for data in data_channel {
        println!("Received data: {}", data);
        rflow::send(format!("command accepted: {}", data));
        let command = data.trim();
        if command == "quit" {
            break;
        }
    }
    Ok(())
}
```
