# ily-rs
rewrite of ily using socketioxide (cuz the server is heavily underpowered so)

it's a chat website you can self host that uses socketioxide for the server and socketio on the client.

This chat website exists because its minimal and easy to set up and use. I created this because all chat websites were blocked on school-issued chromebook except for the school affiliated one, and that is heavily monitored.

Note that it uses a CDN for socket.io. Please set it up locally if that is an issue.

## Setup

```bash
git clone https://github.com/om-raheja/ily-rs
cd ily-rs
```

First, get a PostgreSQL database URL and store it in the environment variable `DATABASE_URL` in your `.env` file.

Get `sqlx-cli` to set up the database.

```bash
cargo install sqlx-cli
sqlx database create
sqlx migrate run
```

You can add users using the `adduser` command with no arguments.

```bash
cargo run --bin adduser
```

Once you created one (or two) users, you can run the server, login, and start messaging.

```bash
cargo build --release
```

```bash
cargo run --release
```

You can also download the binaries for these actions off of GitHub releases.
