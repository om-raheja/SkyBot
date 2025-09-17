# SkyBot
skybot rewritten for the fourth time (we've been doing this since 2021!) for signal

## Installation
First download `signal-cli`. I personally use it on debian [from here](https://packaging.gitlab.io/signal-cli/installation/standalone/). I use the native binary.

Create a systemd service for `signal-cli` as shown below:

```
[Unit]
Description=Signal Messenger (User Daemon)
After=dbus.service
Requires=dbus.service

[Service]
Type=dbus
ExecStart=/usr/bin/signal-cli -a [...] daemon --dbus
BusName=org.asamk.Signal
Restart=on-failure
RestartSec=3
Environment=XDG_RUNTIME_DIR=/run/user/%U
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus

[Install]
WantedBy=default.target
```

Git clone skybot:

```sh
git clone https://github.com/om-raheja/SkyBot.git
cd SkyBot
```

Use `cargo run` to build and start the bot for testing. Then use the binary provided in a systemd service for production.

## Using the bot

The bot has limited functionality:
- `!help` informs the user that the bot is useless
- `!tag oh` responds with a really long **OHOHOHO...** message...
- `!bal [user]` shows your balance or the balance of the user (don't mention them, use the name SkyBot uses for them which you can find when they run `!bal`)
- `!give [amount] [user]` gives [amount] of Skybucks to the user
- `!issue [amount] [user]` issues [amount] of Skybucks to the user (doesn't withdraw from your balance)
- `!list` shows the phone numbers SkyBot has (your name likely wont show up)
- `!ym` pulls a joke from YoMama API.
