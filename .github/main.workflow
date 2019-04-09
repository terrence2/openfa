workflow "OnPush" {
  on = "push"
  resolves = [
    "fmt-clippy-test",
  ]
}

action "fmt-clippy-test" {
  uses = "icepuma/rust-action@master"
  args = "cargo fmt --all -- --check && cargo clippy --all -- -Dwarnings && cargo test --all"
}
