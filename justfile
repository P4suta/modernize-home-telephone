set shell := ["sh", "-eu", "-c"]

default:
    @just --list

check:
    mise x -- cargo xtask check

api:
    mise x -- cargo xtask api --update

security:
    mise x -- cargo xtask security

lab:
    mise x -- cargo xtask lab

package-inputs:
    mise x -- cargo test --locked -p xtask --features upstream-archives --test cached_packages

verification example:
    mise x -- cargo run --locked -p xtask --features upstream-archives --example {{ quote(example) }}

browser:
    mise x -- cargo xtask browser

deployment:
    mise x -- cargo xtask rehearse

proof:
    mise x -- cargo xtask verify proof

miri:
    mise x -- cargo xtask verify miri

fuzz:
    mise x -- cargo xtask verify fuzz

mutants:
    mise x -- cargo xtask verify mutants

web-mutants:
    mise x -- cargo xtask verify web-mutants

evidence:
    mise x -- cargo xtask evidence

release-check: check security package-inputs lab browser deployment proof miri fuzz mutants web-mutants evidence

package output:
    mise x -- cargo xtask package --output {{ quote(output) }}
