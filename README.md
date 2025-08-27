# ily-rs
rewrite of ily using socketioxide (cuz the server is heavily underpowered so)

it's a chat website you can self host that uses socketioxide for the server and socketio on the client.

This chat website exists because its minimal and easy to set up and use. I created this because all chat websites were blocked on school-issued chromebook except for the school affiliated one, and that is heavily monitored.

## Install

first you want to get `socketio.min.js` from an official CDN (there is no guarantee that the CDN will be unblocked on locked devices. hence, selfhost & proxy as much as you can)

```bash
git clone https://github.com/om-raheja/ily-rs
cargo build --release
mkdir -p html/socket.io/
curl https://cdn.socket.io/4.8.1/socket.io.min.js -o html/socket.io/socketio.js

