<h2>
  RFlow
  <a href="https://crates.io/crates/rflow"><img alt="crates.io page" src="https://img.shields.io/crates/v/rflow.svg"></img></a>
  <a href="https://docs.rs/rflow"><img alt="docs.rs page" src="https://docs.rs/rflow/badge.svg"></img></a>
</h2>

Chat-like HMI for embedded Rust applications and PLCs

## The idea

Many of embedded applications and PLC programs do not have any human-machine
interface and not supposed to have one by design.

However, sometimes it is very useful to have a simple way to interact with the
application, e.g. for debugging purposes or having a basic emergency interface
in production.

RFlow provides the most possible lightweight way to have a chat-like interface
between the application (server) and its clients, which does not affect the
application real-time run-flow and consumes minimal system resources.

The [RFlow protocol](blob/main/protocol.md) is fully text-based and can be used
with no special client.

## Clients

* [RFlow Chat](https://crates.io/crates/rflow-chat) - a dedicated RFlow chat
  client (terminal).

* Custom clients, built with the crate `Client` API.

* Any terminal TCP client, e.g. `telnet`, `nc`.
