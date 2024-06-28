# Protocol description

RFlow is a text-based protocol that is used to communicate between the client
and the server. The protocol is designed to be simple and easy to implement.

## Server greeting

When the client connects to the server, the server sends a greeting message:

```
RFLOW/1
[optional headers]
---
```

* `RFLOW/1` is the protocol name and current version.
* `[optional headers]` is a list of optional headers that the server can send
  (HEADER: VALUE).
* `---` header transmission separator.

## Client to server messages

All messages SHOULD be sent as a single line. Messages from clients to server
are sent as-is.

## Server to client messages

All messages SHOULD be sent as a single line. Messages from server to clients
are prefixed as:

* `<<<` a message, sent by the server itself
* `>>>` a message, sent by the current (echo) or another client

## End of connection

When the client disconnects, the connection is closed.
