#!/bin/sh
sed -i 's/^\(version =\).*/\1 "'"$(git describe --tags | sed 's/^.//')"'"/' Cargo.toml Cargo.windows.toml
