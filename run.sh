#!/usr/bin/env bash
cargo wasi build --example test $1 && cargo run --example test $1
