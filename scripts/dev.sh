#!/usr/bin/bash -e
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
mkdir -p $SCRIPT_DIR/tmp
#export RUST_LOG=debug

function run() {
    cd "$SCRIPT_DIR" && cargo run -- --config $SCRIPT_DIR/dev-config.toml --allow-posting-insecure-connections "$@"
}

function admin() {
    run admin "$@"
}

case "$1" in
    init)
        admin add-group local.general
        admin add-group local.moderated --moderated
        admin add-user admin admin
    ;;
    clean)
        rm -r ${SCRIPT_DIR}/tmp
    ;;
    server)
        echo "To connect to this server use a newsreader"
        echo "To be able to post use the credentials admin:admin"
        echo "Example:"
        echo "  tin -r -g localhost:1119"
        echo "  tin -r -g localhost:1119 -A # to force login"
        run
    ;;
esac
