#!/bin/sh
set -eu

cargo check -p appcore-ai --all-targets --no-default-features
cargo check -p appcore-ai --all-targets --no-default-features --features accelerator-nvidia
cargo check -p appcore-ai --all-targets --no-default-features --features backend-candle
cargo check -p appcore-ai --all-targets --no-default-features --features backend-openai-compatible
cargo check -p appcore-ai --all-targets --no-default-features --features training-candle
cargo check -p appcore-ai --all-targets --no-default-features --features swarm
cargo check -p appcore-ai --all-targets --all-features
cargo check -p appcore-bin --all-targets --features ai-alpha

if cargo tree -p appcore-ai --no-default-features -e normal | grep -Eq 'candle-(core|nn)'; then
    echo "Candle leaked into the appcore-ai default dependency graph" >&2
    exit 1
fi

if cargo tree -p appcore-ai --no-default-features -e normal | grep -Eq 'appcore-transport|base64|serde_json'; then
    echo "OpenAI-compatible transport dependencies leaked into the appcore-ai default graph" >&2
    exit 1
fi

if cargo tree -p appcore-ai --no-default-features -e normal | grep -Eq 'nvml-wrapper'; then
    echo "NVIDIA discovery dependencies leaked into the appcore-ai default graph" >&2
    exit 1
fi
