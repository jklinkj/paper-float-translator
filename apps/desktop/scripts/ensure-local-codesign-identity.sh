#!/usr/bin/env bash
set -euo pipefail

IDENTITY_NAME="${LOCAL_CODESIGN_IDENTITY:-Paper Float Translator Local Codesign}"
KEYCHAIN="${LOCAL_CODESIGN_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

if [[ "${LOCAL_CODESIGN_RECREATE:-0}" != "1" ]] &&
  security find-identity -v -p codesigning | grep -F "\"$IDENTITY_NAME\"" >/dev/null; then
  echo "$IDENTITY_NAME"
  exit 0
fi

if security find-certificate -c "$IDENTITY_NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  security delete-certificate -c "$IDENTITY_NAME" "$KEYCHAIN" >/dev/null 2>&1 || true
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

OPENSSL_CONFIG="$TMP_DIR/codesign.cnf"
KEY_PATH="$TMP_DIR/local-codesign.key"
CERT_PATH="$TMP_DIR/local-codesign.crt"

cat >"$OPENSSL_CONFIG" <<EOF
[ req ]
distinguished_name = req_distinguished_name
x509_extensions = v3_codesign
prompt = no

[ req_distinguished_name ]
CN = $IDENTITY_NAME

[ v3_codesign ]
basicConstraints = critical,CA:true
keyUsage = critical,digitalSignature,keyCertSign
extendedKeyUsage = codeSigning
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer
EOF

openssl req \
  -x509 \
  -newkey rsa:2048 \
  -nodes \
  -days 3650 \
  -keyout "$KEY_PATH" \
  -out "$CERT_PATH" \
  -config "$OPENSSL_CONFIG" \
  >/dev/null 2>&1

security import "$KEY_PATH" \
  -k "$KEYCHAIN" \
  -T /usr/bin/codesign \
  -T /usr/bin/security \
  >/dev/null

security import "$CERT_PATH" \
  -k "$KEYCHAIN" \
  -T /usr/bin/codesign \
  -T /usr/bin/security \
  >/dev/null

security add-trusted-cert \
  -r trustRoot \
  -p codeSign \
  -k "$KEYCHAIN" \
  "$CERT_PATH" \
  >/dev/null

if [[ -n "${LOCAL_CODESIGN_KEYCHAIN_PASSWORD:-}" ]]; then
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$LOCAL_CODESIGN_KEYCHAIN_PASSWORD" \
    "$KEYCHAIN" \
    >/dev/null
fi

if ! security find-identity -v -p codesigning | grep -F "\"$IDENTITY_NAME\"" >/dev/null; then
  echo "failed to create local codesigning identity: $IDENTITY_NAME" >&2
  exit 65
fi

echo "$IDENTITY_NAME"
