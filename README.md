# QUIC Tunnel

Tunnel UDP or TCP over a client-cert authenticated QUIC tunnel.

I'm on an airplane and the packet loss is terrible. For SSH, I use mosh, but my other services are bad too. One day, VPNs and video streaming apps will probably use QUIC on their own and this won't be needed.

Long ago I found UDPSpeeder, but it makes bandwidth usage worse. The retrying built into QUIC along with NewReno congestion control should work well in a high latency, low bandwidth, high loss network.

Would a combination of stunnel/socat/iptables be enough? Are there other similar tools? Probably, but I want to code something for fun to play with QUIC and maybe io_uring.

## Usage

### Create Certificates

Create some self-signed certificates:

    cargo run --bin certs data first

For more complicated (and secure) certificates, you can use other tools like [mkcert](https://github.com/FiloSottile/mkcert).

### DNS Tunnel

Start the server:

    cargo run --bin udp_server data/ca.pem data/server.pem data/server.key.pem 127.0.0.1:8053 1.1.1.1:53

Start the client:

    cargo run --bin udp_client data/ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:18053 127.0.0.1:8053

Test the client:

    dig example.com @127.0.0.1 -p 18053

### WireGuard Tunnel

Under construction.

Start the wireguard server:

    ...

Start the server (locally for testing):

    cargo run --bin udp_server data/ca.pem data/server.pem data/server.key.pem 127.0.0.1:51819 "$wireguard_server_ip:51820"

Start the tunnel client (locally for testing):

    cargo run --bin udp_client data/ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:51818 127.0.0.1:51819

Configure the wireguard client:

 - instead of `$wireguard_server_ip:51820`, connect to `127.0.0.1:51818`

### TCP Reverse Proxy

...

### TCP Proxy

...

## Todo

- [ ] client certs are disabled. re-enable them
- [ ] cute name
- [ ] cute mascot
- [ ] tokio-iouring feature
- [ ] compression? mixing encryption and compression are very difficult to do securely
- [ ] translate docs to match places with airplane-quality internet connections
- [ ] keepalive/timeouts aren't working properly