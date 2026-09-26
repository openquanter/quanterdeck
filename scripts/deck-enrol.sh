#!/bin/sh
# Enrol this machine in a deck by proving an SSH key.
#
# For a machine you have an SSH key on and no deck password on: it asks
# the deck for a challenge, signs it with a key the deck's
# OQ_DECK_TRUSTED_KEYS file lists, and prints a link. Open the link in
# the browser you want remembered, and that browser is enrolled.
#
# Nothing here is a secret and nothing here is durable: the challenge is
# a nonce that is spent by the attempt, the claim code is good once for
# two minutes, and what the browser ends up holding is a device
# credential you can revoke from the deck's Settings page.
#
#   ./deck-enrol.sh --url http://127.0.0.1:8899 --key ~/.ssh/id_ed25519
#   ./deck-enrol.sh --url https://deck.example --identity dingzg@macmini
#
# Over SSH, put the deck's port on this machine first:
#   ssh -N -L 8899:127.0.0.1:8899 huawei &
#   ./deck-enrol.sh

set -eu

URL=${OQ_DECK_URL:-http://127.0.0.1:8899}
KEY=${OQ_DECK_KEY:-$HOME/.ssh/id_ed25519}
IDENTITY=
LABEL=

while [ $# -gt 0 ]; do
    case $1 in
        --url) URL=$2; shift 2 ;;
        --key) KEY=$2; shift 2 ;;
        --identity) IDENTITY=$2; shift 2 ;;
        --label) LABEL=$2; shift 2 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

# The principal the deck looks up in its allowed_signers file. Defaults to
# this machine and this user, which is what an operator would write there.
IDENTITY=${IDENTITY:-$(id -un)@$(hostname -s)}
LABEL=${LABEL:-$IDENTITY}

for need in curl ssh-keygen python3; do
    command -v "$need" >/dev/null 2>&1 || {
        echo "$0 needs $need, which is not on PATH" >&2
        exit 1
    }
done
[ -f "$KEY" ] || { echo "no such key: $KEY" >&2; exit 1; }

URL=${URL%/}

# python3 for the JSON: the signature is armored text with newlines and
# header lines in it, and hand-quoting that in shell is how a script
# works until the day a key has an unusual comment in it.
field() { python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }

echo "asking $URL for a challenge" >&2
challenge=$(curl -fsS -X POST "$URL/api/v1/session/challenge" \
    -H 'content-type: application/json' -d '{}') || {
    echo "the deck did not answer. Is it running, and is the tunnel up?" >&2
    exit 1
}
namespace=$(printf '%s' "$challenge" | field namespace)
nonce=$(printf '%s' "$challenge" | field challenge)

# The signature file ssh-keygen writes sits beside the key and is removed
# on the way out, including on failure.
sigfile=$(mktemp "${TMPDIR:-/tmp}/deck-enrol.XXXXXX")
trap 'rm -f "$sigfile"' EXIT INT TERM
printf '%s' "$nonce" | ssh-keygen -Y sign -n "$namespace" -f "$KEY" - >"$sigfile"

echo "signed as $IDENTITY; asking the deck to check it" >&2
body=$(python3 -c '
import json, sys
print(json.dumps({
    "challenge": sys.argv[1],
    "signature": open(sys.argv[2]).read(),
    "identity": sys.argv[3],
    "label": sys.argv[4],
}))' "$nonce" "$sigfile" "$IDENTITY" "$LABEL")

answer=$(curl -fsS -X POST "$URL/api/v1/session/enrol" \
    -H 'content-type: application/json' -d "$body") || {
    echo "the deck refused the signature. Most often the key is not in its" >&2
    echo "OQ_DECK_TRUSTED_KEYS file, or identity ($IDENTITY) is not the name" >&2
    echo "it is listed under there." >&2
    exit 1
}
claim=$(printf '%s' "$answer" | field claim)

echo >&2
echo "open this in the browser you want remembered — it is good for two" >&2
echo "minutes and for one browser:" >&2
echo >&2
printf '%s/enrol?code=%s\n' "$URL" "$claim"
