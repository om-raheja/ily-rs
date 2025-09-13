# ily-rs
rewrite of ily using socketioxide (cuz the server is heavily underpowered so)

it's a chat website you can self host that uses socketioxide for the server and socketio on the client.

This chat website exists because its minimal and easy to set up and use. I created this because all chat websites were blocked on school-issued chromebook except for the school affiliated one, and that is heavily monitored.

## Note 
This project may not be actively maintained going forward.

Here are the issues I had before I shut it down:
- sometimes the server would restart oddly. just put print between each statement. i think it oculd be a systemctl problem
- channels doesn't fully work on the frontend. just use the commit before the channels one 

## Install

first you want to get `socketio.min.js` from an official CDN (there is no guarantee that the CDN will be unblocked on locked devices. hence, selfhost & proxy as much as you can)

```bash
git clone https://github.com/om-raheja/ily-rs
cargo build --release
mkdir -p html/socket.io/
curl https://cdn.socket.io/4.8.1/socket.io.min.js -o html/socket.io/socketio.js

