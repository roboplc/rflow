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

MSRV: 1.68.0

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

## Locking safety

Note: the asynchronous client uses `parking_lot_rt` locking only.

By default, the crate (both the server and the client modules) uses
[parking_lot](https://crates.io/crates/parking_lot) for locking. For real-time
applications, the following features are available:

* `locking-rt` - use [parking_lot_rt](https://crates.io/crates/parking_lot_rt)
  crate which is a spin-free fork of parking_lot.

* `locking-rt-safe` - use [rtsc](https://crates.io/crates/rtsc)
  priority-inheritance locking, which is not affected by priority inversion
  (Linux only).

Note: to switch locking policy, disable the crate default features.

## About

RFlow is a part of [RoboPLC](https://www.roboplc.com/) project.
