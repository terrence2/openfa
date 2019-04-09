workflow "OnPush" {
  on = "push"
  resolves = [
    "Build Docker Build Env",
    "fmt-clippy-test",
  ]
}

action "Build Docker Build Env" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
  args = "build -t openfa:latest ."
}

action "fmt-clippy-test" {
  uses = "icepuma/rust-action@master"
  args = "make ci"
}
