workflow "OnPush" {
  on = "push"
  resolves = [
    "fmt-clippy-test",
  ]
}

action "fmt-clippy-test" {
  uses = "icepuma/rust-action@master"
}
