#!/usr/bin/env bash
cargo wasi build --example test $* && cargo run --example test $*
