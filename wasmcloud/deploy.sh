#!/bin/bash

# List of zones to deploy to
declare -a ZONES=(
  "betty-edge-k8s"
  # "betty-acceptance-k8s"
  # "betty-ca4-k8s"
  # "betty-cc2-k8s"
  # "betty-chiesi-k8s"
  # "betty-frasers-k8s"
  # "betty-holygrow-k8s"
  # "betty-meditel-k8s"
  # "betty-nh1816-k8s"
  # "betty-nl3-k8s"
  # "betty-nl4-k8s"
  # "betty-nl6-k8s"
  # "betty-pfl-k8s"
  # "betty-pluryn-k8s"
  # "betty-police-k8s"
  # "betty-rva-k8s"
  # "betty-sanofi-k8s"
  # "betty-spc1-k8s"
  # "betty-trial-k8s"
  # "betty-us2-k8s"
  # "betty-uwm-k8s"
  # "betty-vabi-k8s"
)

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)

status_native_app() {
  wash app list -o json | jq -r --arg zone "$1" '{status: .applications[] | select(.name == "native").status, zone: $zone}'
}

deploy_native_app() {
  wash app deploy -o json "$SCRIPT_DIR/prod.wadm.yaml" 2>&1 | jq -r --arg zone "$1" '. += {zone: $zone}'
}

purge_native_app() {
  wash app delete native
  wash app deploy "$SCRIPT_DIR/prod.wadm.yaml"
}

run_for_each_zone() {
  local callback=$1

  for zone in "${ZONES[@]}"; do
    kubectl -n wasm-apps --cluster "$zone" port-forward svc/wasmcloud-host 4222:4222 >/dev/null 2>&1 &
    pid=$!
    sleep 5s

    $callback "$zone"

    kill $pid
  done
}

# Now you can call it with different callbacks
run_for_each_zone status_native_app
# run_for_each_zone deploy_native_app
# run_for_each_zone purge_native_app
