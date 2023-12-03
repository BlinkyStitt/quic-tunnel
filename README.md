# QUIC Tunnel

Tunnel UDP or TCP over a client-cert authenticated QUIC tunnel.

I'm on an airplane and the packet loss is terrible. For SSH, I use mosh, but my other services are bad too. One day, VPNs and video streaming apps will probably use QUIC on their own and this won't be needed.

Long ago I found UDPSpeeder, but it makes bandwidth usage worse. The retrying built into QUIC along with NewReno congestion control should work well in a high latency, low bandwidth, high loss network.

Would a combination of stunnel/socat/iptables be enough? Probably, but I want to code something for fun to play with QUIC and maybe io_uring.

## DNS Forwarding

Create some self-signed certificates:

    ```
    cargo run --bin certs data first
    ```

Start the server:

    ```
    cargo run --bin udp_server data/ca.pem data/server.pem data/server.key.pem 127.0.0.1:8053 1.1.1.1:53
    ```

Start the client 

    ```
    cargo run --bin udp_client data/ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:18053 127.0.0.1:8053

    ```

## TODO

- [ ] tokio-iouring feature
- [ ] compression? mixing encryption and compression are very difficult to do securely
- [ ] client certs are disabled. re-enable them
