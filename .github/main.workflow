workflow "OnPush" {
  on = "push"
  resolves = ["Build OpenFA"]
}

action "Build Docker Build Env" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
  args = "build -t openfa:latest ."
}

action "Build OpenFA" {
  uses = "actions/docker/cli@8cdf801b322af5f369e00d85e9cf3a7122f49108"
  needs = ["Build Docker Build Env"]
  args = "run -t openfa:latest"
}
