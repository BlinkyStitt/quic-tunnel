# QUIC Tunnel

Tunnel UDP or TCP over a client-cert authenticated QUIC tunnel.

Would something just for ssl and some iptables rules work, too? probably this is easier for **me**.

## DNS Forwarding

Create some self-signed certificates:

    ```
    cargo run --bin certs data first
    ```

Start the server:

    ```
    cargo run --bin server data/ca.pem data/server.pem data/server.key.pem 127.0.0.1:8053 1.1.1.1:53
    ```

Start the client 

    ```
    cargo run --bin client

    ```

## TODO

- [ ] tokio-iouring feature
