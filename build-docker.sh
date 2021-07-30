#!/bin/sh
exec docker build -t rustup -f dockerfiles/Dockerfile.ubuntu .
