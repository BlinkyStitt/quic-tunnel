# QUIC Tunnel

Tunnel UDP or TCP over a client-cert authenticated QUIC tunnel.

You should probably use [Cloudflare's VPN](https://warp.plus/zxVp1) instead. It works very similarly to this.

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

    cargo run --bin udp_server data/first_ca.pem data/first_server.pem data/first_server.key.pem 127.0.0.1:8053 1.1.1.1:53

Start the client:

    cargo run --bin udp_client data/first_ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:18053 127.0.0.1:8053

Test the client:

    dig example.com @127.0.0.1 -p 18053

### WireGuard Tunnel

Under construction. I need to figure out the `route add` command to run.

Start the wireguard server:

    ...

Start the server (locally for testing):

    cargo run --bin udp_server data/first_ca.pem data/first_server.pem data/server.key.pem 127.0.0.1:51819 "$wireguard_server_ip:51820"

Start the tunnel client (locally for testing):

    cargo run --bin udp_client data/first_ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:51818 127.0.0.1:51819

Configure the wireguard client:

 - instead of `$wireguard_server_ip:51820`, connect to `127.0.0.1:51818`

### TCP Reverse Proxy

Start your app listening on TCP. For this example, it will be a simple docker container:

    docker run --rm -p 8080:80 --name quic-tunnel-example nginx

This test curl command will go directly to nginx:

    curl localhost:8080

Start the tunnel server:

    cargo run --bin reverse_proxy_server data/first_ca.pem data/first_server.pem data/first_server.key.pem 127.0.0.1:8443 127.0.0.1:18080

Start the tunnel client:

    cargo run --bin reverse_proxy_client data/first_ca.pem data/first_client.pem data/first_client.key.pem 127.0.0.1:8080 127.0.0.1:8443

This test curl command will go through the server to the client and finally to the nginx docker container:

    curl localhost:18080

### TCP Proxy

...

### TUN/TAP device

...

### Unix Socket

...

## Todo

- [x] keepalive/timeouts aren't working properly
- [x] client cert
- [x] compression? mixing encryption and compression are very difficult to do securely
- [ ] cute name
- [ ] cute mascot
- [ ] tokio-iouring feature
- [ ] translate docs to match places with airplane-quality internet connections
- [ ] Instead of running Wireguard on top of this tunnel, use boringtun and run wireguard in this process
- [ ] single binary for all commands
- [ ] run in a cloudflare edge worker (or similar) on demand
