workflow "OnPush" {
  on = "push"
  resolves = [
    "cargo fmt",
    "cargo clippy",
    "cargo test",
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