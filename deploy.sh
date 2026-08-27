
# ssh bronze-magpie-prod


# aarch64-unknown-linux-gnu

# rustup target add aarch64-unknown-linux-musl
# brew install messense/macos-cross-toolchains/aarch64-unknown-linux-musl
cargo build --target aarch64-unknown-linux-musl --release

scp target/aarch64-unknown-linux-musl/release/bronze-magpie bronze-magpie-prod:~/bronze-magpie.new
ssh bronze-magpie-prod \
"pkill bronze-magpie || true; \
   mv ~/bronze-magpie.new ~/bronze-magpie; \
   chmod +x ~/bronze-magpie; \
   sudo setcap 'cap_net_bind_service=+ep' ~/bronze-magpie; \
   nohup ./bronze-magpie > /dev/null 2>&1 &"