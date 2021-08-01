# KodiKontroller for Kodi

Copyright Erkki Seppälä <erkki.seppala@vincit.fi> 2021

Licensed under the [MIT license](LICENSE.MIT).

![Screenshot of KodiKontroller playing a file](doc/screenshot.png)

KodiKontrol is a program for streaming local files to
[Kodi](https://kodi.tv/).

It works by starting a web server on a dynamically allocated port and
hosting the files listed from the command line on that server; then it
instructs the Kodi instance to pick those files as its playlist and
then provides a simple terminal interface for controlling the
playback.

A simple IP-based access control is implemented, so only the Kodi
instance provided is able to access the files.

## Binaries

Binaries for Ubuntu, MacOS X and Windows are available in the [GitHub releases
page](../../releases/latest/).

Only the Ubuntu binary has been tested to function (on Debian).

## Building in Ubuntu

And probably Debian.

1) 
```
sudo apt-get install ca-certificates curl file \
    build-essential autoconf automake autotools-dev \
	libtool xutils-dev libssl-dev git pkg-config \
	libncurses-dev
```

2) Install the Rust compiler+Cargo e.g. with https://rustup.rs/

3) `git clone --recursive https://github.com/eras/KodiKontrol`

This step is required due to [Cargo not supporting relative submodule
paths](https://github.com/rust-lang/cargo/issues/7992), and I would
prefer to use them in this case.

4) `cargo install --path KodiKontrol`

5) `$HOME/.cargo/bin/koko` has now been installed

## Usage

To run the interactive setup do:

`% koko --setup`

To run with the default instance:

`% koko *.mp4`

To define another address, use

`% koko -k mykodi foo.mp4`

If such a label is found from config, it's used, otherwise normal host
name resolving is applied. Note: domain search name does not work on
Windows, you need to enter complete host name
(e.g. `hostname.localdomain`).

IP addresses are also permitted. User/pass parameters are functional,
but it's pretty useless with Kodi as they affect only the HTTP
interface. `koko` does use the HTTP interface as well for API reasons,
but also uses the WebSocket API which [doesn't use
authentication](https://kodi.tv/article/kodi-remote-access-security-recommendations/).

`--help` works.

### Shortcuts

| Key        | Function                                                                        |
|------------|---------------------------------------------------------------------------------|
| [/PageUp   | Previous entry in playlist or the beginning of current one.                     |
| ]/PageDown | Next entry in playlist                                                          |
| ,          | Short seek backwards                                                            |
| .          | Short seek forwards                                                             |
| <          | Long seek backwards                                                             |
| >          | Long seek forwards                                                              |
| space      | Play/pause                                                                      |
| q          | Quit                                                                            |
| -, 0-9     | Enter [-]hh:mm:ss (starting from seconds) for a relative seek. Also 5m42 works. |

### Config file

Refer to [the example config file](koko.ini.example).
