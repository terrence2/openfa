workflow "OnPush" {
  on = "push"
  resolves = [
    "cargo fmt",
    "cargo clippy",
    "cargo test",
    "build tools",
  ]
}

action "cargo fmt" {
  uses = "icepuma/rust-action@master"
  args = "cargo fmt --all -- --check"
}

action "cargo clippy" {
  uses = "icepuma/rust-action@master"
  args = "cargo clippy --all -- -Dwarnings"
}

action "cargo test" {
  uses = "icepuma/rust-action@master"
  args = "cargo test --all"
}

action "build tools" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
  args = "build -t openfa:latest ."
}