workflow "Build" {
  on = "push"
  resolves = ["Clippy", "Build action"]
}

action "Build action" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
}

action "Clippy" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
  args = "make clippy"
}
