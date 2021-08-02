(Get-Content "Cargo.toml") -replace ".*#_FOR_WINDOWS ","" >Cargo.toml.tmp
Move-Item -Path Cargo.toml.tmp -Destination Cargo.toml -Force
