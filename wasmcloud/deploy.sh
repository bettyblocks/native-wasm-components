#!/bin/bash

# List of zones to deploy to
## generate this list by running `kubectl config view -o json | jq '.contexts[] | select(.name | test("betty-")) | .name'`
declare -a ZONES=(
"betty-acceptance-k8s"
"betty-ca4-k8s"
"betty-cc2-k8s"
"betty-chiesi-k8s"
"betty-edge-k8s"
"betty-frasers-k8s"
"betty-holygrow-k8s"
"betty-meditel-k8s"
"betty-nh1816-k8s"
"betty-nl3-k8s"
"betty-nl4-k8s"
"betty-nl6-k8s"
"betty-pfl-k8s"
"betty-pluryn-k8s"
"betty-police-k8s"
"betty-rva-k8s"
"betty-sanofi-k8s"
"betty-spc1-k8s"
"betty-trial-k8s"
"betty-us2-k8s"
"betty-uwm-k8s"
"betty-vabi-k8s"
)

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)


# shellcheck disable=SC2120
get_config_json() {
  zone=$1
  if [ -z "$zone" ]; then
    kubectl exec -q -n frontend -it svc/assets-api -- ./bin/assets_api rpc 'Application.get_env(:assets_api, :registry) |> Jason.encode! |> IO.puts'
  else
    kubectl exec -q --cluster "$zone" -n frontend -it svc/assets-api -- ./bin/assets_api rpc 'Application.get_env(:assets_api, :registry) |> Jason.encode! |> IO.puts'
  fi
}

push_image() {
  local image=$1
  local path=$2
  if [ -z "$image" ] || [ -z "$path" ]; then
    echo "Usage: push_image <image> <path>"
    return 1
  fi

  config_json=$(get_config_json)
  (
  WASH_REG_URL=$(echo "$config_json" | jq -r .host)
  WASH_REG_PASSWORD=$(echo "$config_json" | jq -r .password)
  WASH_REG_USER=$(echo "$config_json" | jq -r .user)
  export WASH_REG_URL WASH_REG_PASSWORD WASH_REG_USER

  echo "WASH_REG_URL: $WASH_REG_URL"
  echo "WASH_REG_PASSWORD: $WASH_REG_PASSWORD"
  echo "WASH_REG_USER: $WASH_REG_USER"

  wash push data-api:0.1.4-test3 /home/thomas/Betty/actions-providers/providers/data-api/build/data-api.par.gz
  )

}

zone_to_registry() {
  get_config_json "$1" | jq -r .host
}

zone_to_keyvault_endpoint() {
  echo "do it"
}

render_wadm_template() {
  registry=$(zone_to_registry "$1")
  export VERSION=0.2.1
  export DATA_API_IMAGE=$registry/data-api:0.1.0
  export KEY_VAULT_IMAGE=$registry/key-vault:0.1.0
  export KEY_VAULT_ENDPOINT=https://betty-edge-keyvault.vault.azure.net/
  bun run ./template
}

status_native_app() {
  wash app list -o json | jq -r --arg zone "$1" '{status: .applications[] | select(.name == "native").status, zone: $zone}'
}

list_apps() {
  # wash app list -o json | jq --arg zone "$1" '.applications | {apps: map(.name), count: length, zone: $zone}'
  wash app list -o json | jq --arg zone "$1" '.applications | {count: length, zone: $zone}'
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
# run_for_each_zone list_apps
# run_for_each_zone status_native_app
# run_for_each_zone deploy_native_app
# run_for_each_zone purge_native_app


# push_image data-api:0.1.4-test3 /home/thomas/Betty/actions-providers/providers/data-api/build/data-api.par.gz
render_wadm_template betty-acceptance-k8s